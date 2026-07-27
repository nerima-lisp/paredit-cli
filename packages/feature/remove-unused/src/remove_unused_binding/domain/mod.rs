//! Use-case helpers for removing unused let bindings.

use anyhow::{Context, Result};

use paredit_core_edit::mutation_safety::reject_common_lisp_reader_conditionals;
use paredit_core_syntax::common_lisp::{
    CommonLispBindingRefactorForm, common_lisp_dynamic_binding_is_declared,
    common_lisp_symbol_reference_eq, is_common_lisp_earmuffed_special_variable_name,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    Delimiter, ExpressionKind, ExpressionView, Formatter, SymbolName, SyntaxTree,
};

mod core;
mod rewrite;
mod syntax;
#[cfg(test)]
mod tests;
mod types;

use core::{binding_reference_spans, binding_removal_candidates};
use rewrite::{apply_nested_span_edits, replace_span};
use syntax::atom_text;
use types::{RemoveUnusedBindingParts, RemovedBindingParts};
pub use types::{RemoveUnusedBindingPlan, RemoveUnusedBindingRequest, RemovedBindingPlan};

pub fn plan_remove_unused_binding(
    request: RemoveUnusedBindingRequest<'_>,
) -> Result<RemoveUnusedBindingPlan> {
    if request.name.is_some() && request.all_bindings {
        anyhow::bail!("remove-unused-binding accepts either --name or --all-bindings, not both");
    }
    if request.name.is_none() && !request.all_bindings {
        anyhow::bail!("remove-unused-binding requires --name or --all-bindings");
    }
    if request.dialect == Dialect::Unknown {
        anyhow::bail!("remove-unused-binding does not support dialect unknown");
    }

    let input_tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("remove-unused-binding input is not a valid S-expression document")?;
    reject_common_lisp_reader_conditionals(&input_tree, request.dialect)?;
    let parsed_target = input_tree
        .select_at(request.target.span.start().get())
        .context("remove-unused-binding target span is not present in the input")?;
    anyhow::ensure!(
        parsed_target.view() == request.target,
        "remove-unused-binding target does not match the input"
    );

    let parts = remove_unused_binding_parts(
        request.dialect,
        request.input,
        &request.target,
        request.name,
        request.all_bindings,
    )?;
    let rewritten = replace_span(request.input, parts.form_span, &parts.replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("remove-unused-binding output is not a valid S-expression document")?;

    let bindings = parts
        .bindings
        .iter()
        .map(|binding| RemovedBindingPlan {
            binding_name: binding.name.clone(),
            binding_span: binding.binding_span,
            binding_value: binding.binding_value.clone(),
            reference_count: binding.reference_spans.len(),
        })
        .collect::<Vec<_>>();
    let first_binding = bindings.first();

    Ok(RemoveUnusedBindingPlan {
        dialect: request.dialect,
        path: request.path,
        form: parts.form,
        form_span: parts.form_span,
        binding_name: first_binding.map(|binding| binding.binding_name.clone()),
        binding_span: first_binding.map(|binding| binding.binding_span),
        binding_value: first_binding.map(|binding| binding.binding_value.clone()),
        reference_count: first_binding.map(|binding| binding.reference_count),
        bindings,
        dropped_value_requires_review: !request.allow_drop_value,
        replacement: parts.replacement,
        changed: rewritten != request.input,
        rewritten,
    })
}

fn remove_unused_binding_parts(
    dialect: paredit_core_syntax::dialect::Dialect,
    input: &str,
    target: &ExpressionView,
    name: Option<&SymbolName>,
    all_bindings: bool,
) -> Result<RemoveUnusedBindingParts> {
    if target.kind != ExpressionKind::List || target.delimiter != Some(Delimiter::Paren) {
        anyhow::bail!(
            "remove-unused-binding selection must be a let, let*, symbol-macrolet, flet, labels, macrolet, compiler-macrolet, with-slots, with-accessors, do, do*, prog, or prog* list"
        );
    }
    if target.children.len() < 3 {
        anyhow::bail!(
            "remove-unused-binding requires a supported binding form with bindings and a body"
        );
    }
    let head = atom_text(&target.children[0])
        .context("remove-unused-binding form must start with an atom")?;
    let Some(refactor_form) = dialect.common_lisp_binding_refactor_form_for_head(head) else {
        anyhow::bail!(
            "remove-unused-binding selection must start with let, let*, symbol-macrolet, flet, labels, macrolet, compiler-macrolet, with-slots, with-accessors, do, do*, prog, or prog*"
        );
    };
    if !refactor_form.supports_remove_unused_binding() {
        anyhow::bail!(
            "remove-unused-binding selection must start with let, let*, symbol-macrolet, flet, labels, macrolet, compiler-macrolet, with-slots, with-accessors, do, do*, prog, or prog*"
        );
    }
    if matches!(refactor_form, CommonLispBindingRefactorForm::Slot(_)) && target.children.len() < 4
    {
        anyhow::bail!(
            "remove-unused-binding requires a with-slots or with-accessors form with bindings, an instance expression, and a body"
        );
    }
    if matches!(refactor_form, CommonLispBindingRefactorForm::Do(_)) && target.children.len() < 3 {
        anyhow::bail!(
            "remove-unused-binding requires a do or do* form with bindings and an end clause"
        );
    }
    ensure_variable_binding_form_consistency(dialect, head, refactor_form)?;

    let binding_form = &target.children[1];
    let candidates = binding_removal_candidates(dialect, refactor_form, binding_form)?;
    let input_tree = SyntaxTree::parse_with_dialect(input, dialect)
        .context("remove-unused-binding input is not a valid S-expression document")?;
    let selected = if all_bindings {
        let mut unused = Vec::new();
        for candidate in &candidates {
            let symbol = SymbolName::new(candidate.name.clone())?;
            let reference_spans = binding_reference_spans(
                dialect,
                input,
                target,
                refactor_form,
                binding_form,
                &candidates,
                candidate,
                &symbol,
            )?;
            // An earmuffed (`*name*`) name with zero lexical references is,
            // by the near-universal Common Lisp convention, very likely a
            // rebind of a `defvar`/`defparameter`-declared special
            // variable — meaningful purely through its dynamic-scope side
            // effect for the body's dynamic extent, e.g. `(let
            // ((*read-eval* nil)) (read stream))`. `--all-bindings` must
            // not silently delete that: doing so can change program
            // behavior instead of removing dead code. Skip it from bulk
            // selection; `--name` still allows removing it explicitly.
            // The same holds for a value binding whose name is dynamically
            // declared special elsewhere in the document, even without the
            // earmuff naming convention. This is restricted to forms that
            // actually introduce dynamic-scopeable value bindings (let/do/
            // prog) so a same-named local function or macro binding (e.g.
            // `(flet ((dynamic () 1)) ...)`) is never mistaken for a
            // rebind of a `defvar`'d variable of the same name — those live
            // in separate namespaces.
            if reference_spans.is_empty()
                && !(dialect == Dialect::CommonLisp
                    && (is_common_lisp_earmuffed_special_variable_name(&candidate.name)
                        || (refactor_form.supports_dynamic_special_binding()
                            && common_lisp_dynamic_binding_is_declared(
                                &input_tree.root_view(),
                                target,
                                &symbol,
                            ))))
            {
                unused.push(RemovedBindingParts {
                    name: candidate.name.clone(),
                    binding_span: candidate.removal_span,
                    binding_value: candidate.value_span.slice(input).to_owned(),
                    reference_spans,
                });
            }
        }
        if unused.is_empty() {
            anyhow::bail!("remove-unused-binding --all-bindings found no unused bindings");
        }
        unused
    } else {
        let name = name.ok_or_else(|| {
            anyhow::anyhow!("remove-unused-binding requires --name or --all-bindings")
        })?;
        let candidate = candidates
            .iter()
            .find(|candidate| binding_name_matches(dialect, &candidate.name, name.as_str()))
            .with_context(|| {
                format!(
                    "binding {} was not found in selected binding form",
                    name.as_str()
                )
            })?;
        let reference_spans = binding_reference_spans(
            dialect,
            input,
            target,
            refactor_form,
            binding_form,
            &candidates,
            candidate,
            name,
        )?;
        let reference_count = reference_spans.len();
        if reference_count != 0 {
            anyhow::bail!(
                "remove-unused-binding requires zero in-scope references; found {reference_count}"
            );
        }
        vec![RemovedBindingParts {
            name: candidate.name.clone(),
            binding_span: candidate.removal_span,
            binding_value: candidate.value_span.slice(input).to_owned(),
            reference_spans,
        }]
    };

    let preserve_binding_form_when_empty = refactor_form.preserves_binding_form_when_empty();
    let body_start_index = refactor_form.remove_unused_body_start_index();
    let replacement = if selected.len() == candidates.len() && !preserve_binding_form_when_empty {
        let first_body = &target.children[body_start_index];
        let last_body = target.children.last().context(
            "remove-unused-binding expected at least one body expression after validation",
        )?;
        paredit_core_syntax::sexpr::ByteSpan::new(first_body.span.start(), last_body.span.end())
            .slice(input)
            .to_owned()
    } else {
        let replacement = apply_nested_span_edits(
            target.span.slice(input),
            target.span,
            selected
                .iter()
                .map(|binding| (binding.binding_span, String::new()))
                .collect(),
        )?;
        format_single_replacement_form(&replacement, dialect)?
    };

    Ok(RemoveUnusedBindingParts {
        form: head.to_owned(),
        form_span: target.span,
        bindings: selected,
        replacement,
    })
}

fn ensure_variable_binding_form_consistency(
    dialect: paredit_core_syntax::dialect::Dialect,
    head: &str,
    refactor_form: CommonLispBindingRefactorForm,
) -> Result<()> {
    let expected = match refactor_form {
        CommonLispBindingRefactorForm::Do(form) | CommonLispBindingRefactorForm::Prog(form) => form,
        _ => return Ok(()),
    };

    let Some(actual) = dialect.variable_binding_form_for_head(head) else {
        anyhow::bail!("remove-unused-binding could not classify variable binding form");
    };
    if actual != expected {
        anyhow::bail!("remove-unused-binding variable binding classification mismatch");
    }
    Ok(())
}

fn binding_name_matches(dialect: Dialect, candidate: &str, expected: &str) -> bool {
    match dialect {
        Dialect::CommonLisp => common_lisp_symbol_reference_eq(candidate, expected),
        Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Clojure
        | Dialect::Janet
        | Dialect::Fennel => candidate == expected,
        Dialect::Unknown => false,
    }
}

fn format_single_replacement_form(input: &str, dialect: Dialect) -> Result<String> {
    let tree = SyntaxTree::parse_with_dialect(input, dialect)
        .context("remove-unused-binding replacement is not a valid S-expression form")?;
    if tree.root_children().len() != 1 {
        anyhow::bail!("remove-unused-binding replacement must contain exactly one form");
    }

    let mut formatted = Formatter::new(2).format(&tree);
    if formatted.ends_with('\n') {
        formatted.pop();
    }
    Ok(formatted)
}
