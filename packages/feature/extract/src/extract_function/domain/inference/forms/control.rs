use crate::domain::sexpr::ExpressionView;

use super::{
    ExtractFunctionSemantic, extend_extract_function_bound_params, iteration_spec_bound_name,
    iteration_spec_init_form, iteration_spec_step_form, push_extract_function_bound_param,
};

pub(super) fn collect_inferred_extract_function_do(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
    sequential_scope: bool,
) -> bool {
    let Some(binding_form) = view.children.get(1) else {
        return false;
    };

    let body_bound_params = if sequential_scope {
        collect_sequential_do_inits(
            semantic,
            binding_form,
            explicit_params,
            bound_params,
            params,
        )
    } else {
        collect_parallel_do_inits(
            semantic,
            binding_form,
            explicit_params,
            bound_params,
            params,
        )
    };

    for spec in &binding_form.children {
        if let Some(step_form) = iteration_spec_step_form(spec) {
            super::super::collect_inferred_extract_function_params(
                semantic,
                step_form,
                false,
                explicit_params,
                &body_bound_params,
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
            &body_bound_params,
            params,
        );
    }
    true
}

pub(super) fn collect_inferred_extract_function_prog(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
    sequential_scope: bool,
) -> bool {
    let Some(binding_form) = view.children.get(1) else {
        return false;
    };

    let body_bound_params = if sequential_scope {
        collect_sequential_do_inits(
            semantic,
            binding_form,
            explicit_params,
            bound_params,
            params,
        )
    } else {
        collect_parallel_do_inits(
            semantic,
            binding_form,
            explicit_params,
            bound_params,
            params,
        )
    };

    for body in &view.children[2..] {
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

fn collect_sequential_do_inits(
    semantic: ExtractFunctionSemantic,
    binding_form: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) -> Vec<String> {
    let mut body_bound_params = bound_params.to_vec();
    for spec in &binding_form.children {
        if let Some(init_form) = iteration_spec_init_form(spec) {
            super::super::collect_inferred_extract_function_params(
                semantic,
                init_form,
                false,
                explicit_params,
                &body_bound_params,
                params,
            );
        }
        if let Some(name) = iteration_spec_bound_name(spec) {
            push_extract_function_bound_param(semantic, &mut body_bound_params, name);
        }
    }
    body_bound_params
}

fn collect_parallel_do_inits(
    semantic: ExtractFunctionSemantic,
    binding_form: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) -> Vec<String> {
    for spec in &binding_form.children {
        if let Some(init_form) = iteration_spec_init_form(spec) {
            super::super::collect_inferred_extract_function_params(
                semantic,
                init_form,
                false,
                explicit_params,
                bound_params,
                params,
            );
        }
    }
    extend_extract_function_bound_params(
        semantic,
        bound_params,
        binding_form
            .children
            .iter()
            .filter_map(iteration_spec_bound_name),
    )
}
