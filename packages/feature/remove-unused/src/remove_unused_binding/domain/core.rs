use anyhow::{Context, Result};

use crate::domain::common_lisp::{
    CommonLispBindingListShape, CommonLispBindingRefactorForm, CommonLispLocalCallableForm,
    CommonLispSlotBindingForm, CommonLispVariableSpecForm,
};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView};

#[derive(Debug)]
pub(crate) struct LetBindingRemovalCandidate {
    pub(crate) index: usize,
    pub(crate) name: String,
    pub(crate) value_span: ByteSpan,
    pub(crate) removal_span: ByteSpan,
}

pub(crate) fn binding_removal_candidates(
    dialect: Dialect,
    refactor_form: CommonLispBindingRefactorForm,
    binding_form: &ExpressionView,
) -> Result<Vec<LetBindingRemovalCandidate>> {
    if matches!(dialect, Dialect::Clojure | Dialect::Janet | Dialect::Fennel) {
        return vector_let_binding_removal_candidates(binding_form);
    }

    let Some(shape) = refactor_form.binding_list_shape() else {
        anyhow::bail!("remove-unused-binding does not support this Common Lisp binding form");
    };
    match shape {
        CommonLispBindingListShape::NameValuePairs => {
            list_pair_let_binding_removal_candidates(binding_form)
        }
        CommonLispBindingListShape::LocalCallableDefinitions(form) => {
            list_pair_local_callable_binding_removal_candidates(binding_form, form)
        }
        CommonLispBindingListShape::VariableSpecs(form) => {
            list_pair_iteration_binding_removal_candidates(binding_form, form)
        }
        CommonLispBindingListShape::SlotBindings(form) => {
            list_pair_slot_binding_removal_candidates(binding_form, form)
        }
    }
}

fn vector_let_binding_removal_candidates(
    binding_form: &ExpressionView,
) -> Result<Vec<LetBindingRemovalCandidate>> {
    if binding_form.kind != ExpressionKind::List
        || binding_form.delimiter != Some(Delimiter::Bracket)
    {
        anyhow::bail!("dialect expects vector let bindings: [name value ...]");
    }
    if binding_form.children.len() % 2 != 0 {
        anyhow::bail!("vector let binding form must contain name/value pairs");
    }

    binding_form
        .children
        .chunks_exact(2)
        .enumerate()
        .map(|(index, pair)| {
            let name = atom_text(&pair[0])
                .context("let binding name must be an atom")?
                .to_owned();
            Ok(LetBindingRemovalCandidate {
                index,
                name,
                value_span: pair[1].span,
                removal_span: ByteSpan::new(pair[0].span.start(), pair[1].span.end()),
            })
        })
        .collect()
}

fn list_pair_iteration_binding_removal_candidates(
    binding_form: &ExpressionView,
    form: CommonLispVariableSpecForm,
) -> Result<Vec<LetBindingRemovalCandidate>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        let form_name = form.form_name();
        anyhow::bail!("dialect expects {form_name} bindings: (variable-spec ...)");
    }

    binding_form
        .children
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            let (name, value_span) = iteration_variable_spec_name_and_value_span(spec, form)?;
            Ok(LetBindingRemovalCandidate {
                index,
                name: name.to_owned(),
                value_span,
                removal_span: spec.span,
            })
        })
        .collect()
}

fn iteration_variable_spec_name_and_value_span(
    spec: &ExpressionView,
    form: CommonLispVariableSpecForm,
) -> Result<(&str, ByteSpan)> {
    if spec.kind == ExpressionKind::Atom {
        let name = atom_text(spec).context("iteration binding name must be an atom")?;
        return Ok((name, spec.span));
    }

    if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
        let form_name = form.form_name();
        anyhow::bail!("{form_name} binding must be a symbol or variable spec list");
    }
    if spec.children.is_empty() || spec.children.len() > form.max_children() {
        let form_name = form.form_name();
        anyhow::bail!("{form_name} variable spec has an unsupported arity");
    }

    let name = atom_text(&spec.children[0]).context("iteration binding name must be an atom")?;
    let value_span = spec
        .children
        .get(1)
        .map_or(spec.children[0].span, |child| child.span);
    Ok((name, value_span))
}

fn list_pair_slot_binding_removal_candidates(
    binding_form: &ExpressionView,
    form: CommonLispSlotBindingForm,
) -> Result<Vec<LetBindingRemovalCandidate>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        match form {
            CommonLispSlotBindingForm::WithSlots => {
                anyhow::bail!("dialect expects with-slots bindings: (slot-or-pair ...)");
            }
            CommonLispSlotBindingForm::WithAccessors => {
                anyhow::bail!("dialect expects with-accessors bindings: ((name accessor) ...)");
            }
        }
    }

    binding_form
        .children
        .iter()
        .enumerate()
        .map(|(index, spec)| match form {
            CommonLispSlotBindingForm::WithSlots => slot_binding_removal_candidate(index, spec),
            CommonLispSlotBindingForm::WithAccessors => {
                accessor_binding_removal_candidate(index, spec)
            }
        })
        .collect()
}

fn slot_binding_removal_candidate(
    index: usize,
    spec: &ExpressionView,
) -> Result<LetBindingRemovalCandidate> {
    if spec.kind == ExpressionKind::Atom {
        let name = atom_text(spec)
            .context("with-slots bare binding name must be an atom")?
            .to_owned();
        return Ok(LetBindingRemovalCandidate {
            index,
            name,
            value_span: spec.span,
            removal_span: spec.span,
        });
    }
    if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
        anyhow::bail!("with-slots binding must be a slot name or (name slot-name) pair");
    }
    if spec.children.len() != 2 {
        anyhow::bail!("with-slots binding pair must contain a name and slot name");
    }
    let name = atom_text(&spec.children[0])
        .context("with-slots binding name must be an atom")?
        .to_owned();
    Ok(LetBindingRemovalCandidate {
        index,
        name,
        value_span: spec.children[1].span,
        removal_span: spec.span,
    })
}

fn accessor_binding_removal_candidate(
    index: usize,
    spec: &ExpressionView,
) -> Result<LetBindingRemovalCandidate> {
    if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
        anyhow::bail!("with-accessors binding must be a (name accessor) pair");
    }
    if spec.children.len() != 2 {
        anyhow::bail!("with-accessors binding pair must contain a name and accessor");
    }
    let name = atom_text(&spec.children[0])
        .context("with-accessors binding name must be an atom")?
        .to_owned();
    Ok(LetBindingRemovalCandidate {
        index,
        name,
        value_span: spec.children[1].span,
        removal_span: spec.span,
    })
}

fn list_pair_let_binding_removal_candidates(
    binding_form: &ExpressionView,
) -> Result<Vec<LetBindingRemovalCandidate>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        anyhow::bail!("dialect expects list-pair let bindings: ((name value) ...)");
    }

    binding_form
        .children
        .iter()
        .enumerate()
        .map(|(index, pair)| {
            if pair.kind != ExpressionKind::List || pair.delimiter != Some(Delimiter::Paren) {
                if pair.kind != ExpressionKind::Atom {
                    anyhow::bail!("let binding must be a name, (name), or (name value)");
                }
                let name = atom_text(pair)
                    .context("let binding name must be an atom")?
                    .to_owned();
                return Ok(LetBindingRemovalCandidate {
                    index,
                    name,
                    value_span: pair.span,
                    removal_span: pair.span,
                });
            }
            if pair.children.is_empty() || pair.children.len() > 2 {
                anyhow::bail!("let binding pair must be (name) or (name value)");
            }
            let name = atom_text(&pair.children[0])
                .context("let binding name must be an atom")?
                .to_owned();
            let value_span = pair
                .children
                .get(1)
                .map_or(pair.children[0].span, |value| value.span);
            Ok(LetBindingRemovalCandidate {
                index,
                name,
                value_span,
                removal_span: pair.span,
            })
        })
        .collect()
}

fn list_pair_local_callable_binding_removal_candidates(
    binding_form: &ExpressionView,
    form: CommonLispLocalCallableForm,
) -> Result<Vec<LetBindingRemovalCandidate>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        anyhow::bail!(
            "dialect expects list-pair {} bindings: ((name lambda-list form*) ...)",
            form.operator_name()
        );
    }
    let body_label = if form.is_macro() {
        "macro expander body"
    } else {
        "lambda list, and body"
    };

    binding_form
        .children
        .iter()
        .enumerate()
        .map(|(index, pair)| {
            if pair.kind != ExpressionKind::List || pair.delimiter != Some(Delimiter::Paren) {
                anyhow::bail!(
                    "{} binding must be a (name lambda-list form*) list",
                    form.operator_name()
                );
            }
            if pair.children.len() < 2 {
                anyhow::bail!(
                    "{} binding must contain a name and {}",
                    form.operator_name(),
                    body_label
                );
            }
            let name = atom_text(&pair.children[0])
                .with_context(|| format!("{} binding name must be an atom", form.operator_name()))?
                .to_owned();
            let value_start = pair.children[1].span.start();
            let value_end = pair
                .children
                .last()
                .with_context(|| {
                    format!(
                        "{} binding must contain a name and {}",
                        form.operator_name(),
                        body_label
                    )
                })?
                .span
                .end();
            Ok(LetBindingRemovalCandidate {
                index,
                name,
                value_span: ByteSpan::new(value_start, value_end),
                removal_span: pair.span,
            })
        })
        .collect()
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

mod references;

pub(crate) use references::binding_reference_spans;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        dialect::Dialect,
        sexpr::{SymbolName, SyntaxTree},
    };

    fn target_for(input: &str) -> (SyntaxTree, ExpressionView) {
        let tree = SyntaxTree::parse(input).expect("input should parse");
        let target = tree.root_view().children[0].clone();
        (tree, target)
    }

    #[test]
    fn collects_common_lisp_binding_candidates_and_references() {
        let (_tree, target) = target_for("(let* ((first 1) (second first)) second)");
        let form = Dialect::CommonLisp
            .common_lisp_binding_refactor_form_for_head("let*")
            .expect("let* should be supported");
        let candidates = binding_removal_candidates(Dialect::CommonLisp, form, &target.children[1])
            .expect("bindings should parse");

        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        let name = SymbolName::new("first".to_owned()).expect("symbol should parse");
        let references = binding_reference_spans(
            Dialect::CommonLisp,
            "(let* ((first 1) (second first)) second)",
            &target,
            form,
            &target.children[1],
            &candidates,
            &candidates[0],
            &name,
        )
        .expect("references should collect");

        assert_eq!(references.len(), 1);
        assert_eq!(references[0].len(), "first".len());
    }

    #[test]
    fn collects_clojure_vector_binding_candidates() {
        let (_, target) = target_for("(let [first 1 second 2] second)");
        let form = Dialect::Clojure
            .common_lisp_binding_refactor_form_for_head("let")
            .expect("let should be supported");
        let candidates = binding_removal_candidates(Dialect::Clojure, form, &target.children[1])
            .expect("vector bindings should parse");

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].name, "first");
        assert_eq!(candidates[1].name, "second");
    }
}
