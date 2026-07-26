use std::iter;

use crate::domain::common_lisp::CommonLispResourceBindingForm;
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};

use super::super::super::syntax::atom_text;
use super::super::bindings::extract_function_binding_entries;
use super::super::patterns::parameter_names;
use super::{ExtractFunctionSemantic, extend_extract_function_bound_params, slot_spec_bound_name};

pub(super) fn collect_inferred_extract_function_let(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) -> bool {
    let Some(binding_form) = view.children.get(1) else {
        return false;
    };
    if binding_form.delimiter == Some(Delimiter::Bracket) {
        return collect_inferred_extract_function_let_star(
            semantic,
            view,
            explicit_params,
            bound_params,
            params,
        );
    }
    let Some(bindings) = extract_function_binding_entries(semantic, binding_form) else {
        return false;
    };

    for binding in &bindings {
        if let Some(value) = &binding.value {
            super::super::collect_inferred_extract_function_params(
                semantic,
                value,
                false,
                explicit_params,
                bound_params,
                params,
            );
        }
    }

    let body_bound_params = extend_extract_function_bound_params(
        semantic,
        bound_params,
        bindings
            .iter()
            .flat_map(|binding| binding.names.iter().map(String::as_str)),
    );
    collect_bodies(
        semantic,
        &view.children[2..],
        explicit_params,
        &body_bound_params,
        params,
    );
    true
}

pub(super) fn collect_inferred_extract_function_let_star(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) -> bool {
    let Some(binding_form) = view.children.get(1) else {
        return false;
    };
    let Some(bindings) = extract_function_binding_entries(semantic, binding_form) else {
        return false;
    };

    let mut current_bound_params = bound_params.to_vec();
    for binding in &bindings {
        if let Some(value) = &binding.value {
            super::super::collect_inferred_extract_function_params(
                semantic,
                value,
                false,
                explicit_params,
                &current_bound_params,
                params,
            );
        }
        for name in &binding.names {
            super::push_extract_function_bound_param(semantic, &mut current_bound_params, name);
        }
    }

    collect_bodies(
        semantic,
        &view.children[2..],
        explicit_params,
        &current_bound_params,
        params,
    );
    true
}

pub(super) fn collect_inferred_extract_function_value_binding(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) -> bool {
    let Some(binding_form) = view.children.get(1) else {
        return false;
    };
    let Some(value_form) = view.children.get(2) else {
        return false;
    };

    super::super::collect_inferred_extract_function_params(
        semantic,
        value_form,
        false,
        explicit_params,
        bound_params,
        params,
    );

    let names = parameter_names(semantic, binding_form);
    let body_bound_params = extend_extract_function_bound_params(
        semantic,
        bound_params,
        names.iter().map(String::as_str),
    );
    collect_bodies(
        semantic,
        &view.children[3..],
        explicit_params,
        &body_bound_params,
        params,
    );
    true
}

pub(super) fn collect_inferred_extract_function_clause_form(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) -> bool {
    let Some(protected_form) = view.children.get(1) else {
        return false;
    };

    super::super::collect_inferred_extract_function_params(
        semantic,
        protected_form,
        false,
        explicit_params,
        bound_params,
        params,
    );

    for clause in &view.children[2..] {
        if clause.kind != ExpressionKind::List || clause.delimiter != Some(Delimiter::Paren) {
            super::super::collect_inferred_extract_function_params(
                semantic,
                clause,
                false,
                explicit_params,
                bound_params,
                params,
            );
            continue;
        }

        let Some(parameter_form) = clause.children.get(1) else {
            continue;
        };
        let names = parameter_names(semantic, parameter_form);
        let clause_bound_params = extend_extract_function_bound_params(
            semantic,
            bound_params,
            names.iter().map(String::as_str),
        );
        collect_bodies(
            semantic,
            &clause.children[2..],
            explicit_params,
            &clause_bound_params,
            params,
        );
    }
    true
}

pub(super) fn collect_inferred_extract_function_handler_bind(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
    include_restart_options: bool,
) -> bool {
    let Some(binding_form) = view.children.get(1) else {
        return false;
    };

    for spec in &binding_form.children {
        if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
            continue;
        }

        if let Some(function_form) = spec.children.get(1) {
            super::super::collect_inferred_extract_function_params(
                semantic,
                function_form,
                false,
                explicit_params,
                bound_params,
                params,
            );
        }

        if include_restart_options {
            collect_inferred_extract_function_restart_option_values(
                semantic,
                spec,
                explicit_params,
                bound_params,
                params,
            );
        }
    }

    collect_bodies(
        semantic,
        &view.children[2..],
        explicit_params,
        bound_params,
        params,
    );
    true
}

fn collect_inferred_extract_function_restart_option_values(
    semantic: ExtractFunctionSemantic,
    spec: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) {
    let mut index = 2;
    while index + 1 < spec.children.len() {
        super::super::collect_inferred_extract_function_params(
            semantic,
            &spec.children[index + 1],
            false,
            explicit_params,
            bound_params,
            params,
        );
        index += 2;
    }
}

pub(super) fn collect_inferred_extract_function_iteration_binding(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) -> bool {
    let Some(binding_form) = view.children.get(1) else {
        return false;
    };

    if let Some(source_form) = binding_form.children.get(1) {
        super::super::collect_inferred_extract_function_params(
            semantic,
            source_form,
            false,
            explicit_params,
            bound_params,
            params,
        );
    }

    let body_bound_params = extend_extract_function_bound_params(
        semantic,
        bound_params,
        binding_form
            .children
            .first()
            .and_then(super::atom_text)
            .into_iter(),
    );

    if let Some(result_form) = binding_form.children.get(2) {
        super::super::collect_inferred_extract_function_params(
            semantic,
            result_form,
            false,
            explicit_params,
            &body_bound_params,
            params,
        );
    }

    collect_bodies(
        semantic,
        &view.children[2..],
        explicit_params,
        &body_bound_params,
        params,
    );
    true
}

pub(super) fn collect_inferred_extract_function_slot_binding(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) -> bool {
    let Some(slot_specs) = view.children.get(1) else {
        return false;
    };
    let Some(instance_form) = view.children.get(2) else {
        return false;
    };

    super::super::collect_inferred_extract_function_params(
        semantic,
        instance_form,
        false,
        explicit_params,
        bound_params,
        params,
    );

    let body_bound_params = extend_extract_function_bound_params(
        semantic,
        bound_params,
        slot_specs.children.iter().filter_map(slot_spec_bound_name),
    );
    collect_bodies(
        semantic,
        &view.children[3..],
        explicit_params,
        &body_bound_params,
        params,
    );
    true
}

fn collect_bodies(
    semantic: ExtractFunctionSemantic,
    bodies: &[ExpressionView],
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) {
    for body in bodies {
        super::super::collect_inferred_extract_function_params(
            semantic,
            body,
            false,
            explicit_params,
            bound_params,
            params,
        );
    }
}

pub(super) fn collect_inferred_extract_function_resource_binding(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
    resource_form: CommonLispResourceBindingForm,
) -> bool {
    let Some(binding_spec) = view.children.get(1) else {
        return false;
    };
    let Some(binding_name) = binding_spec.children.first().and_then(atom_text) else {
        return false;
    };

    for initializer in binding_spec.children.iter().skip(1) {
        super::super::collect_inferred_extract_function_params(
            semantic,
            initializer,
            false,
            explicit_params,
            bound_params,
            params,
        );
    }

    let body_bound_params =
        extend_extract_function_bound_params(semantic, bound_params, iter::once(binding_name));
    for body_form in view.children.iter().skip(resource_form.body_start_index()) {
        super::super::collect_inferred_extract_function_params(
            semantic,
            body_form,
            false,
            explicit_params,
            &body_bound_params,
            params,
        );
    }

    true
}
