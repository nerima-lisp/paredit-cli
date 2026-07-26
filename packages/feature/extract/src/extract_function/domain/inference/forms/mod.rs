mod bindings;
mod callable;
mod control;

use crate::domain::common_lisp::{CommonLispLetBindingForm, CommonLispValueScopeForm};
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};

use super::super::syntax::{atom_text, list_head};
use super::ExtractFunctionSemantic;

pub(super) fn collect_inferred_extract_function_special_form(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) -> bool {
    let dialect = semantic.dialect();
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return false;
    }

    let Some(head) = list_head(view) else {
        return false;
    };

    let Some(scope_form) = dialect.common_lisp_value_scope_form_for_head(head) else {
        return false;
    };

    match scope_form {
        CommonLispValueScopeForm::Let(CommonLispLetBindingForm::Parallel)
        | CommonLispValueScopeForm::Let(CommonLispLetBindingForm::SymbolMacro) => {
            bindings::collect_inferred_extract_function_let(
                semantic,
                view,
                explicit_params,
                bound_params,
                params,
            )
        }
        CommonLispValueScopeForm::Let(CommonLispLetBindingForm::Sequential) => {
            bindings::collect_inferred_extract_function_let_star(
                semantic,
                view,
                explicit_params,
                bound_params,
                params,
            )
        }
        CommonLispValueScopeForm::Value => {
            bindings::collect_inferred_extract_function_value_binding(
                semantic,
                view,
                explicit_params,
                bound_params,
                params,
            )
        }
        CommonLispValueScopeForm::Clause => {
            bindings::collect_inferred_extract_function_clause_form(
                semantic,
                view,
                explicit_params,
                bound_params,
                params,
            )
        }
        CommonLispValueScopeForm::Handler(form) => {
            bindings::collect_inferred_extract_function_handler_bind(
                semantic,
                view,
                explicit_params,
                bound_params,
                params,
                form.includes_restart_options(),
            )
        }
        CommonLispValueScopeForm::Iteration => {
            bindings::collect_inferred_extract_function_iteration_binding(
                semantic,
                view,
                explicit_params,
                bound_params,
                params,
            )
        }
        CommonLispValueScopeForm::Variable(form)
            if dialect.common_lisp_variable_binding_has_step_forms_for_head(head) =>
        {
            control::collect_inferred_extract_function_do(
                semantic,
                view,
                explicit_params,
                bound_params,
                params,
                form.is_sequential(),
            )
        }
        CommonLispValueScopeForm::Variable(form) => {
            control::collect_inferred_extract_function_prog(
                semantic,
                view,
                explicit_params,
                bound_params,
                params,
                form.is_sequential(),
            )
        }
        CommonLispValueScopeForm::Slot => bindings::collect_inferred_extract_function_slot_binding(
            semantic,
            view,
            explicit_params,
            bound_params,
            params,
        ),
        CommonLispValueScopeForm::Resource(resource_form) => {
            bindings::collect_inferred_extract_function_resource_binding(
                semantic,
                view,
                explicit_params,
                bound_params,
                params,
                resource_form,
            )
        }
        CommonLispValueScopeForm::Lambda | CommonLispValueScopeForm::FunctionLiteral => {
            callable::collect_inferred_extract_function_lambda(
                semantic,
                view,
                1,
                explicit_params,
                bound_params,
                params,
            )
        }
        CommonLispValueScopeForm::Definition => callable::collect_inferred_extract_function_lambda(
            semantic,
            view,
            2,
            explicit_params,
            bound_params,
            params,
        ),
        CommonLispValueScopeForm::LocalCallable(_) => {
            callable::collect_inferred_extract_function_local_callable_form(
                semantic,
                view,
                explicit_params,
                bound_params,
                params,
            )
        }
    }
}

pub(super) fn extend_extract_function_bound_params<'a>(
    semantic: ExtractFunctionSemantic,
    bound_params: &[String],
    names: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let mut extended = bound_params.to_vec();
    for name in names {
        push_extract_function_bound_param(semantic, &mut extended, name);
    }
    extended
}

pub(super) fn push_extract_function_bound_param(
    semantic: ExtractFunctionSemantic,
    bound_params: &mut Vec<String>,
    name: &str,
) {
    if super::symbols::is_extract_function_param_candidate(name)
        && !bound_params
            .iter()
            .any(|param| super::extract_function_param_name_eq(semantic, param, name))
    {
        bound_params.push(name.to_owned());
    }
}

pub(super) fn slot_spec_bound_name(slot_spec: &ExpressionView) -> Option<&str> {
    atom_text(slot_spec).or_else(|| slot_spec.children.first().and_then(atom_text))
}

pub(super) fn iteration_spec_bound_name(spec: &ExpressionView) -> Option<&str> {
    atom_text(spec).or_else(|| spec.children.first().and_then(atom_text))
}

pub(super) fn iteration_spec_init_form(spec: &ExpressionView) -> Option<&ExpressionView> {
    (spec.kind == ExpressionKind::List)
        .then(|| spec.children.get(1))
        .flatten()
}

pub(super) fn iteration_spec_step_form(spec: &ExpressionView) -> Option<&ExpressionView> {
    (spec.kind == ExpressionKind::List)
        .then(|| spec.children.get(2))
        .flatten()
}
