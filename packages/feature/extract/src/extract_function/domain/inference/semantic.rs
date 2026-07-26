use crate::domain::dialect::{
    BinderShape, BindingVisibility, BodyShape, DefinitionShape, Dialect, ParameterShape,
    RelativeNodePath, ScopeShape,
};
use crate::domain::sexpr::ExpressionView;

use super::bindings::{ExtractFunctionBindingEntry, extract_function_binding_entries};
use super::forms::{extend_extract_function_bound_params, push_extract_function_bound_param};
use super::patterns::{extract_function_pattern_names, parameter_names};
use super::{ExtractFunctionSemantic, collect_inferred_extract_function_params};

pub(super) fn collect_inferred_extract_function_semantic_form(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) -> bool {
    if semantic.dialect() == Dialect::CommonLisp {
        return false;
    }

    if let Some(scope) = semantic.scope_shape(view) {
        collect_scope(semantic, view, scope, explicit_params, bound_params, params);
        return true;
    }

    if let Some(definition) = semantic.definition_shape(view) {
        collect_definition(
            semantic,
            view,
            definition,
            explicit_params,
            bound_params,
            params,
        );
        return true;
    }

    false
}

fn collect_scope(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    scope: ScopeShape,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) {
    match scope.binders() {
        BinderShape::BindingList {
            container,
            visibility,
            ..
        }
        | BinderShape::FlatPairs {
            container,
            visibility,
            ..
        } => collect_binding_scope(
            semantic,
            view,
            container,
            None,
            visibility,
            scope.body(),
            explicit_params,
            bound_params,
            params,
        ),
        BinderShape::NamedBindingList {
            scope_name,
            container,
            visibility,
            ..
        } => collect_binding_scope(
            semantic,
            view,
            container,
            Some(scope_name),
            visibility,
            scope.body(),
            explicit_params,
            bound_params,
            params,
        ),
        BinderShape::Parameters(parameters) => collect_parameter_scope(
            semantic,
            view,
            None,
            parameters,
            scope.body(),
            explicit_params,
            bound_params,
            params,
        ),
        BinderShape::NamedParameters { name, parameters } => collect_parameter_scope(
            semantic,
            view,
            Some(name),
            parameters,
            scope.body(),
            explicit_params,
            bound_params,
            params,
        ),
        BinderShape::ParameterClauses {
            name,
            first_clause_index,
            parameters,
        } => collect_parameter_clauses(
            semantic,
            view,
            name,
            first_clause_index,
            parameters,
            scope.body(),
            explicit_params,
            bound_params,
            params,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_binding_scope(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    container_path: RelativeNodePath,
    scope_name: Option<RelativeNodePath>,
    visibility: BindingVisibility,
    body: BodyShape,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) {
    let Some(container) = resolve_relative(view, container_path) else {
        return;
    };
    let Some(entries) = extract_function_binding_entries(semantic, container) else {
        return;
    };

    let mut body_bound_params = bound_params.to_vec();
    match visibility {
        BindingVisibility::Parallel => {
            for entry in &entries {
                collect_binding_initializer(semantic, entry, explicit_params, bound_params, params);
            }
            extend_with_binding_names(semantic, &mut body_bound_params, &entries);
        }
        BindingVisibility::Sequential => {
            for entry in &entries {
                collect_binding_initializer(
                    semantic,
                    entry,
                    explicit_params,
                    &body_bound_params,
                    params,
                );
                extend_with_names(semantic, &mut body_bound_params, &entry.names);
            }
        }
    }

    if let Some(scope_name) = scope_name {
        let Some(name) = resolve_relative(view, scope_name) else {
            return;
        };
        extend_with_pattern(semantic, &mut body_bound_params, name);
    }

    collect_body(
        semantic,
        view,
        body,
        explicit_params,
        &body_bound_params,
        params,
    );
}

fn collect_binding_initializer(
    semantic: ExtractFunctionSemantic,
    entry: &ExtractFunctionBindingEntry,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) {
    if let Some(value) = &entry.value {
        collect_inferred_extract_function_params(
            semantic,
            value,
            false,
            explicit_params,
            bound_params,
            params,
        );
    }
}

fn extend_with_binding_names(
    semantic: ExtractFunctionSemantic,
    bound_params: &mut Vec<String>,
    entries: &[ExtractFunctionBindingEntry],
) {
    for entry in entries {
        extend_with_names(semantic, bound_params, &entry.names);
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_parameter_scope(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    name: Option<RelativeNodePath>,
    parameters: ParameterShape,
    body: BodyShape,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) {
    let Some(parameter_names) = parameter_names_at(semantic, view, parameters) else {
        return;
    };
    let mut body_bound_params = extend_extract_function_bound_params(
        semantic,
        bound_params,
        parameter_names.iter().map(String::as_str),
    );

    if let Some(name) = name {
        let Some(name) = resolve_relative(view, name) else {
            return;
        };
        extend_with_pattern(semantic, &mut body_bound_params, name);
    }

    collect_body(
        semantic,
        view,
        body,
        explicit_params,
        &body_bound_params,
        params,
    );
}

#[allow(clippy::too_many_arguments)]
fn collect_parameter_clauses(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    name: Option<RelativeNodePath>,
    first_clause_index: usize,
    parameters: ParameterShape,
    body: BodyShape,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) {
    let BodyShape::ClauseChildrenFrom {
        first_clause_index: body_first_clause_index,
        body_child_index,
    } = body
    else {
        return;
    };
    if first_clause_index != body_first_clause_index {
        return;
    }

    let name = name.and_then(|path| resolve_relative(view, path));
    for clause in view.children.iter().skip(first_clause_index) {
        let Some(parameter_names) = parameter_names_at(semantic, clause, parameters) else {
            return;
        };
        let mut clause_bound_params = extend_extract_function_bound_params(
            semantic,
            bound_params,
            parameter_names.iter().map(String::as_str),
        );
        if let Some(name) = name {
            extend_with_pattern(semantic, &mut clause_bound_params, name);
        }

        for child in clause.children.iter().skip(body_child_index) {
            collect_inferred_extract_function_params(
                semantic,
                child,
                false,
                explicit_params,
                &clause_bound_params,
                params,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_definition(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    definition: DefinitionShape,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) {
    let mut body_bound_params = bound_params.to_vec();
    if let Some(name) = definition.name() {
        let Some(name) = resolve_relative(view, name) else {
            return;
        };
        extend_with_pattern(semantic, &mut body_bound_params, name);
    }
    if let Some(parameters) = definition.parameters() {
        let Some(names) = parameter_names_at(semantic, view, parameters) else {
            return;
        };
        extend_with_names(semantic, &mut body_bound_params, &names);
    }

    collect_body(
        semantic,
        view,
        definition.body(),
        explicit_params,
        &body_bound_params,
        params,
    );
}

fn collect_body(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    body: BodyShape,
    explicit_params: &[String],
    bound_params: &[String],
    params: &mut Vec<String>,
) {
    let first_body_index = match body {
        BodyShape::ChildrenFrom(index) => index,
        BodyShape::ChildrenAfter(path) => path.child() + 1,
        BodyShape::ClauseChildrenFrom { .. } => return,
    };

    for child in view.children.iter().skip(first_body_index) {
        collect_inferred_extract_function_params(
            semantic,
            child,
            false,
            explicit_params,
            bound_params,
            params,
        );
    }
}

fn parameter_names_at(
    semantic: ExtractFunctionSemantic,
    view: &ExpressionView,
    parameters: ParameterShape,
) -> Option<Vec<String>> {
    let parameter_form = resolve_relative(view, parameters.container())?;
    let first_parameter_index = parameters.first_parameter_index();
    if first_parameter_index > parameter_form.children.len() {
        return None;
    }

    let mut parameter_form = parameter_form.clone();
    parameter_form.children.drain(..first_parameter_index);
    Some(parameter_names(semantic, &parameter_form))
}

fn extend_with_pattern(
    semantic: ExtractFunctionSemantic,
    bound_params: &mut Vec<String>,
    pattern: &ExpressionView,
) {
    let names = extract_function_pattern_names(semantic, pattern);
    extend_with_names(semantic, bound_params, &names);
}

fn extend_with_names(
    semantic: ExtractFunctionSemantic,
    bound_params: &mut Vec<String>,
    names: &[String],
) {
    for name in names {
        push_extract_function_bound_param(semantic, bound_params, name);
    }
}

fn resolve_relative(view: &ExpressionView, path: RelativeNodePath) -> Option<&ExpressionView> {
    let child = view.children.get(path.child())?;
    path.grandchild()
        .map_or(Some(child), |grandchild| child.children.get(grandchild))
}
