use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};

use super::super::patterns::parameter_names;
use super::{ExtractFunctionSemantic, extend_extract_function_bound_params};

pub(super) fn collect_inferred_extract_function_lambda(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    parameter_index: usize,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) -> bool {
    let Some(parameter_form) = view.children.get(parameter_index) else {
        return false;
    };
    let names = parameter_names(semantic, parameter_form);
    let body_bound_params = extend_extract_function_bound_params(
        semantic,
        bound_params,
        names.iter().map(String::as_str),
    );
    for body in &view.children[parameter_index + 1..] {
        super::super::collect_inferred_extract_function_params(
            semantic,
            body,
            false,
            explicit_params,
            &body_bound_params,
            params,
        );
    }
    true
}

pub(super) fn collect_inferred_extract_function_local_callable_form(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) -> bool {
    let Some(binding_form) = view.children.get(1) else {
        return false;
    };
    if binding_form.delimiter != Some(Delimiter::Paren) {
        return false;
    }

    for binding in &binding_form.children {
        if binding.kind != ExpressionKind::List || binding.delimiter != Some(Delimiter::Paren) {
            continue;
        }
        let Some(parameter_form) = binding.children.get(1) else {
            continue;
        };
        let lambda_bound_params = extend_extract_function_bound_params(
            semantic,
            bound_params,
            parameter_names(semantic, parameter_form)
                .iter()
                .map(String::as_str),
        );
        for body in binding.children.iter().skip(2) {
            super::super::collect_inferred_extract_function_params(
                semantic,
                body,
                false,
                explicit_params,
                &lambda_bound_params,
                params,
            );
        }
    }

    for body in &view.children[2..] {
        super::super::collect_inferred_extract_function_params(
            semantic,
            body,
            false,
            explicit_params,
            bound_params,
            params,
        );
    }
    true
}
