use anyhow::{Context, Result};

use crate::domain::common_lisp::{
    CommonLispBindingRefactorForm, CommonLispBindingReferenceScope, CommonLispLocalCallableForm,
    CommonLispVariableBindingForm, CommonLispVariableSpecForm, common_lisp_local_callable_form,
    common_lisp_symbol_reference_eq, is_local_callable_bound, local_callable_body_scope,
    local_callable_definition_reference_scope,
};
use crate::domain::definition::definition_shape;
use crate::domain::dialect::Dialect;
use crate::domain::lexical_scope::collect_unshadowed_symbol_references;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, SymbolName};

use super::LetBindingRemovalCandidate;
use crate::domain::remove_unused_binding::syntax::{list_head, view_at_span};

struct BindingReferenceContext<'a> {
    dialect: Dialect,
    input: &'a str,
    target: &'a ExpressionView,
    binding_form: &'a ExpressionView,
    candidates: &'a [LetBindingRemovalCandidate],
    candidate: &'a LetBindingRemovalCandidate,
    name: &'a SymbolName,
}

#[expect(
    clippy::too_many_arguments,
    reason = "binding reference resolution takes the selected binding plus traversal context"
)]
pub(crate) fn binding_reference_spans(
    dialect: Dialect,
    input: &str,
    target: &ExpressionView,
    refactor_form: CommonLispBindingRefactorForm,
    binding_form: &ExpressionView,
    candidates: &[LetBindingRemovalCandidate],
    candidate: &LetBindingRemovalCandidate,
    name: &SymbolName,
) -> Result<Vec<ByteSpan>> {
    let context = BindingReferenceContext {
        dialect,
        input,
        target,
        binding_form,
        candidates,
        candidate,
        name,
    };
    let Some(scope) = refactor_form.reference_scope() else {
        anyhow::bail!("remove-unused-binding unsupported reference scope");
    };

    match scope {
        CommonLispBindingReferenceScope::NameValuePairs(form) => {
            name_value_binding_reference_spans(
                &context,
                form.is_sequential() || binding_form.delimiter == Some(Delimiter::Bracket),
            )
        }
        CommonLispBindingReferenceScope::LocalCallableDefinitions(form) if !form.is_macro() => {
            local_callable_binding_reference_spans(dialect, target, name)
        }
        CommonLispBindingReferenceScope::LocalCallableDefinitions(_) => {
            Ok(body_binding_reference_spans(
                dialect,
                context.input,
                context.target,
                context.name,
                refactor_form.remove_unused_body_start_index(),
            ))
        }
        CommonLispBindingReferenceScope::VariableSpecs(spec_form, binding_form_kind) => Ok(
            variable_spec_binding_reference_spans(&context, spec_form, binding_form_kind),
        ),
        CommonLispBindingReferenceScope::BodyOnly => Ok(body_binding_reference_spans(
            dialect,
            context.input,
            context.target,
            context.name,
            refactor_form.remove_unused_body_start_index(),
        )),
    }
}

fn name_value_binding_reference_spans(
    context: &BindingReferenceContext<'_>,
    sequential_scope: bool,
) -> Result<Vec<ByteSpan>> {
    let mut reference_spans = Vec::new();
    if sequential_scope {
        for later in context
            .candidates
            .iter()
            .filter(|later| later.index > context.candidate.index)
        {
            let later_value = view_at_span(context.binding_form, later.value_span)
                .context("failed to resolve later binding value")?;
            collect_unshadowed_symbol_references(
                context.dialect,
                later_value,
                context.name,
                context.input,
                &mut reference_spans,
            );
        }
    }
    for body in &context.target.children[2..] {
        collect_unshadowed_symbol_references(
            context.dialect,
            body,
            context.name,
            context.input,
            &mut reference_spans,
        );
    }
    Ok(reference_spans)
}

fn variable_spec_binding_reference_spans(
    context: &BindingReferenceContext<'_>,
    spec_form: CommonLispVariableSpecForm,
    binding_form_kind: CommonLispVariableBindingForm,
) -> Vec<ByteSpan> {
    let mut reference_spans = Vec::new();

    if binding_form_kind.is_sequential() {
        for later in context
            .candidates
            .iter()
            .filter(|later| later.index > context.candidate.index)
        {
            let Some(later_spec) = context.binding_form.children.get(later.index) else {
                continue;
            };
            if let Some(init_form) = iteration_variable_spec_init_form(later_spec) {
                collect_unshadowed_symbol_references(
                    context.dialect,
                    init_form,
                    context.name,
                    context.input,
                    &mut reference_spans,
                );
            }
        }
    }

    if spec_form.has_step_forms() {
        for spec in &context.binding_form.children {
            if let Some(step_form) = variable_spec_step_form(spec) {
                collect_unshadowed_symbol_references(
                    context.dialect,
                    step_form,
                    context.name,
                    context.input,
                    &mut reference_spans,
                );
            }
        }
    }

    if let Some(end_clause_index) = spec_form.end_clause_index() {
        if let Some(end_clause) = context.target.children.get(end_clause_index) {
            collect_unshadowed_symbol_references(
                context.dialect,
                end_clause,
                context.name,
                context.input,
                &mut reference_spans,
            );
        }
    }
    for body in context
        .target
        .children
        .iter()
        .skip(spec_form.body_start_index())
    {
        collect_unshadowed_symbol_references(
            context.dialect,
            body,
            context.name,
            context.input,
            &mut reference_spans,
        );
    }

    reference_spans
}

fn body_binding_reference_spans(
    dialect: Dialect,
    input: &str,
    target: &ExpressionView,
    name: &SymbolName,
    body_start_index: usize,
) -> Vec<ByteSpan> {
    let mut reference_spans = Vec::new();
    for body in target.children.iter().skip(body_start_index) {
        collect_unshadowed_symbol_references(dialect, body, name, input, &mut reference_spans);
    }
    reference_spans
}

fn local_callable_binding_reference_spans(
    dialect: Dialect,
    target: &ExpressionView,
    name: &SymbolName,
) -> Result<Vec<ByteSpan>> {
    let mut reference_spans = Vec::new();
    let Some(head) = list_head(target) else {
        return Ok(reference_spans);
    };
    let Some(form) = common_lisp_local_callable_form(dialect, head) else {
        return Ok(reference_spans);
    };

    collect_local_callable_form_reference_spans(
        target,
        dialect,
        name,
        &[],
        form,
        true,
        &mut reference_spans,
    );

    Ok(reference_spans)
}

fn collect_local_callable_form_reference_spans(
    view: &ExpressionView,
    dialect: Dialect,
    name: &SymbolName,
    local_callables: &[String],
    form: CommonLispLocalCallableForm,
    count_this_forms_local_names: bool,
    output: &mut Vec<ByteSpan>,
) {
    let descendant_scope = local_callable_body_scope(local_callables, view);
    let direct_scope = if count_this_forms_local_names {
        local_callables
    } else {
        descendant_scope.as_slice()
    };

    if let Some(bindings) = view.children.get(1) {
        let binding_body_scope = local_callable_definition_reference_scope(
            form,
            direct_scope,
            descendant_scope.as_slice(),
        );
        for binding in &bindings.children {
            for child in binding.children.iter().skip(2) {
                collect_local_callable_reference_spans_from_view(
                    child,
                    dialect,
                    name,
                    binding_body_scope,
                    output,
                );
            }
        }
    }

    for child in view.children.iter().skip(2) {
        collect_local_callable_reference_spans_from_view(
            child,
            dialect,
            name,
            direct_scope,
            output,
        );
    }
}

fn collect_local_callable_reference_spans_from_view(
    view: &ExpressionView,
    dialect: Dialect,
    name: &SymbolName,
    local_callables: &[String],
    output: &mut Vec<ByteSpan>,
) {
    let mut definition_body_range = None;

    if view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren) {
        if let Some(head) = list_head(view) {
            if let Some(form) = common_lisp_local_callable_form(dialect, head) {
                collect_local_callable_form_reference_spans(
                    view,
                    dialect,
                    name,
                    local_callables,
                    form,
                    false,
                    output,
                );
                return;
            }

            if common_lisp_symbol_reference_eq(head, name.as_str())
                && !is_local_callable_bound(local_callables, head)
            {
                if let Some(head_view) = view.children.first() {
                    output.push(head_view.span);
                }
            }

            if let Some(shape) = definition_shape(dialect, view, head) {
                definition_body_range = Some(shape.body_range());
            }
        }
    }

    for (index, child) in view.children.iter().enumerate() {
        if let Some(range) = definition_body_range {
            if !range.contains_child(index) {
                continue;
            }
        }
        collect_local_callable_reference_spans_from_view(
            child,
            dialect,
            name,
            local_callables,
            output,
        );
    }
}

fn iteration_variable_spec_init_form(spec: &ExpressionView) -> Option<&ExpressionView> {
    if spec.kind == ExpressionKind::List {
        spec.children.get(1)
    } else {
        None
    }
}

fn variable_spec_step_form(spec: &ExpressionView) -> Option<&ExpressionView> {
    if spec.kind == ExpressionKind::List {
        spec.children.get(2)
    } else {
        None
    }
}
