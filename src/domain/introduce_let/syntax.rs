use crate::domain::common_lisp::{CommonLispValueScopeForm, common_lisp_symbol_reference_eq};
use crate::domain::dialect::{
    BinderShape, BodyShape, Dialect, IntroduceLetOperation, ParameterShape, RelativeNodePath,
    ScopeShape, VerifiedSemanticPolicy,
};
use crate::domain::sexpr::{Delimiter, ExpressionView};

mod special;

use special::special_declaration_shadows_child;

pub(super) fn binding_form_binds_name(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    view: &ExpressionView,
    name: &str,
) -> bool {
    if semantic_scope_binds_name(semantic, view, name) {
        return true;
    }

    let dialect = semantic.dialect();
    let Some(head) = list_head(view) else {
        return false;
    };

    let form = dialect.common_lisp_value_scope_form_for_head(head);

    match form {
        Some(CommonLispValueScopeForm::Let(_)) => view
            .children
            .get(1)
            .is_some_and(|bindings| binding_pairs_contain_name(bindings, name)),
        Some(CommonLispValueScopeForm::Lambda) => view
            .children
            .get(1)
            .is_some_and(|parameters| lambda_list_contains_name(parameters, name)),
        Some(CommonLispValueScopeForm::FunctionLiteral) => view
            .children
            .get(1)
            .filter(|parameters| parameters.delimiter == Some(Delimiter::Bracket))
            .is_some_and(|parameters| pattern_contains_name(parameters, name)),
        Some(CommonLispValueScopeForm::Definition) => view
            .children
            .get(2)
            .is_some_and(|parameters| lambda_list_contains_name(parameters, name)),
        Some(CommonLispValueScopeForm::Value) => view
            .children
            .get(1)
            .is_some_and(|parameters| lambda_list_contains_name(parameters, name)),
        Some(CommonLispValueScopeForm::Iteration) => view
            .children
            .get(1)
            .is_some_and(|binding_form| iteration_binding_binds_name(binding_form, name)),
        Some(CommonLispValueScopeForm::Variable(_)) => view
            .children
            .get(1)
            .is_some_and(|bindings| variable_specs_bind_name(bindings, name)),
        Some(CommonLispValueScopeForm::Slot) => view
            .children
            .get(1)
            .is_some_and(|slot_specs| slot_specs_bind_name(slot_specs, name)),
        _ => false,
    }
}

pub(super) fn child_shadowed_by_binding(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    view: &ExpressionView,
    name: &str,
    child_index: usize,
) -> bool {
    let dialect = semantic.dialect();
    let Some(head) = list_head(view) else {
        return false;
    };

    let form = dialect.common_lisp_value_scope_form_for_head(head);

    if special_declaration_shadows_child(dialect, view, form, name, child_index) {
        return true;
    }

    if semantic
        .scope_shape(view)
        .is_some_and(|shape| semantic_scope_shadows_child(semantic, view, shape, name, child_index))
        || semantic.definition_shape(view).is_some_and(|shape| {
            body_contains_child(shape.body(), child_index)
                && shape.parameters().is_some_and(|parameters| {
                    parameter_shape_contains_name(semantic, view, parameters, name)
                })
        })
    {
        return true;
    }

    match form {
        Some(CommonLispValueScopeForm::Let(_)) => {
            child_index >= 2 && binding_form_binds_name(semantic, view, name)
        }
        Some(CommonLispValueScopeForm::Lambda | CommonLispValueScopeForm::FunctionLiteral) => {
            child_index >= 2 && binding_form_binds_name(semantic, view, name)
        }
        Some(CommonLispValueScopeForm::Definition) => {
            child_index >= 3 && binding_form_binds_name(semantic, view, name)
        }
        Some(CommonLispValueScopeForm::Value) => {
            child_index >= 3 && binding_form_binds_name(semantic, view, name)
        }
        Some(CommonLispValueScopeForm::Clause) => view
            .children
            .get(child_index)
            .is_some_and(|clause| child_index >= 2 && clause_binds_name(clause, name)),
        Some(CommonLispValueScopeForm::Iteration) => {
            child_index >= 2 && binding_form_binds_name(semantic, view, name)
        }
        Some(CommonLispValueScopeForm::Variable(_)) => {
            child_index >= 2 && binding_form_binds_name(semantic, view, name)
        }
        Some(CommonLispValueScopeForm::Slot) => {
            child_index >= 3 && binding_form_binds_name(semantic, view, name)
        }
        _ => false,
    }
}

fn semantic_scope_binds_name(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    view: &ExpressionView,
    name: &str,
) -> bool {
    let Some(shape) = semantic.scope_shape(view) else {
        return false;
    };

    binder_shape_contains_name(semantic, view, shape.binders(), name)
}

fn semantic_scope_shadows_child(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    view: &ExpressionView,
    shape: ScopeShape,
    name: &str,
    child_index: usize,
) -> bool {
    match (shape.binders(), shape.body()) {
        (
            BinderShape::ParameterClauses {
                name: local_name,
                first_clause_index,
                parameters,
            },
            BodyShape::ClauseChildrenFrom { .. },
        ) if child_index >= first_clause_index => {
            let Some(clause) = view.children.get(child_index) else {
                return false;
            };
            local_name
                .and_then(|path| resolve_relative(view, path))
                .is_some_and(|node| semantic_pattern_contains_name(semantic, node, name))
                || parameter_shape_contains_name(semantic, clause, parameters, name)
        }
        (binders, body) => {
            body_contains_child(body, child_index)
                && binder_shape_contains_name(semantic, view, binders, name)
        }
    }
}

fn binder_shape_contains_name(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    view: &ExpressionView,
    binders: BinderShape,
    name: &str,
) -> bool {
    match binders {
        BinderShape::BindingList {
            container,
            name: path,
            ..
        } => binding_list_contains_name(semantic, view, container, path, name),
        BinderShape::NamedBindingList {
            scope_name,
            container,
            name: path,
            ..
        } => {
            resolve_relative(view, scope_name)
                .is_some_and(|node| semantic_pattern_contains_name(semantic, node, name))
                || binding_list_contains_name(semantic, view, container, path, name)
        }
        BinderShape::FlatPairs {
            container,
            first_name_index,
            stride,
            ..
        } => resolve_relative(view, container).is_some_and(|bindings| {
            bindings
                .children
                .iter()
                .skip(first_name_index)
                .step_by(stride)
                .any(|node| semantic_pattern_contains_name(semantic, node, name))
        }),
        BinderShape::Parameters(parameters) => {
            parameter_shape_contains_name(semantic, view, parameters, name)
        }
        BinderShape::NamedParameters {
            name: local_name,
            parameters,
        } => {
            resolve_relative(view, local_name)
                .is_some_and(|node| semantic_pattern_contains_name(semantic, node, name))
                || parameter_shape_contains_name(semantic, view, parameters, name)
        }
        BinderShape::ParameterClauses {
            name: local_name,
            first_clause_index,
            parameters,
        } => {
            local_name
                .and_then(|path| resolve_relative(view, path))
                .is_some_and(|node| semantic_pattern_contains_name(semantic, node, name))
                || view
                    .children
                    .iter()
                    .skip(first_clause_index)
                    .any(|clause| parameter_shape_contains_name(semantic, clause, parameters, name))
        }
    }
}

fn binding_list_contains_name(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    view: &ExpressionView,
    container: RelativeNodePath,
    name_path: RelativeNodePath,
    name: &str,
) -> bool {
    resolve_relative(view, container).is_some_and(|bindings| {
        bindings.children.iter().any(|binding| {
            resolve_relative(binding, name_path)
                .is_some_and(|node| semantic_pattern_contains_name(semantic, node, name))
        })
    })
}

fn parameter_shape_contains_name(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    view: &ExpressionView,
    parameters: ParameterShape,
    name: &str,
) -> bool {
    resolve_relative(view, parameters.container()).is_some_and(|container| {
        container
            .children
            .iter()
            .skip(parameters.first_parameter_index())
            .any(|parameter| semantic_pattern_contains_name(semantic, parameter, name))
    })
}

fn semantic_pattern_contains_name(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    view: &ExpressionView,
    name: &str,
) -> bool {
    atom_text(view).is_some_and(|text| semantic.identifiers_equal(text, name))
        || view
            .children
            .iter()
            .any(|child| semantic_pattern_contains_name(semantic, child, name))
}

fn resolve_relative(view: &ExpressionView, path: RelativeNodePath) -> Option<&ExpressionView> {
    let child = view.children.get(path.child())?;
    path.grandchild()
        .map_or(Some(child), |grandchild| child.children.get(grandchild))
}

const fn body_contains_child(body: BodyShape, child_index: usize) -> bool {
    match body {
        BodyShape::ChildrenFrom(first) => child_index >= first,
        BodyShape::ChildrenAfter(path) => child_index > path.child(),
        BodyShape::ClauseChildrenFrom {
            first_clause_index, ..
        } => child_index >= first_clause_index,
    }
}

pub(super) fn local_callable_bindings_child_index(
    dialect: Dialect,
    view: &ExpressionView,
) -> Option<usize> {
    matches!(
        dialect.common_lisp_value_scope_form_for_head(list_head(view)?),
        Some(CommonLispValueScopeForm::LocalCallable(_))
    )
    .then_some(1)
}

pub(super) fn let_star_bindings_child_index(
    dialect: Dialect,
    view: &ExpressionView,
) -> Option<usize> {
    matches!(
        dialect.common_lisp_value_scope_form_for_head(list_head(view)?),
        Some(CommonLispValueScopeForm::Let(form)) if form.is_sequential()
    )
    .then_some(1)
}

pub(super) fn iteration_bindings_child_index(
    dialect: Dialect,
    view: &ExpressionView,
) -> Option<usize> {
    matches!(
        dialect.common_lisp_value_scope_form_for_head(list_head(view)?),
        Some(CommonLispValueScopeForm::Iteration)
    )
    .then_some(1)
}

pub(super) fn variable_bindings_child_index(
    dialect: Dialect,
    view: &ExpressionView,
) -> Option<usize> {
    matches!(
        dialect.common_lisp_value_scope_form_for_head(list_head(view)?),
        Some(CommonLispValueScopeForm::Variable(_))
    )
    .then_some(1)
}

pub(super) fn variable_binding_form_is_sequential(dialect: Dialect, view: &ExpressionView) -> bool {
    matches!(
        list_head(view).and_then(|head| dialect.common_lisp_value_scope_form_for_head(head)),
        Some(CommonLispValueScopeForm::Variable(form)) if form.is_sequential()
    )
}

pub(super) fn variable_binding_form_has_step_forms(
    dialect: Dialect,
    view: &ExpressionView,
) -> bool {
    list_head(view)
        .is_some_and(|head| dialect.common_lisp_variable_binding_has_step_forms_for_head(head))
}

pub(super) fn binding_pair_binds_name(binding: &ExpressionView, name: &str) -> bool {
    binding
        .children
        .first()
        .is_some_and(|pattern| pattern_contains_name(pattern, name))
}

pub(super) fn variable_spec_binds_name(binding: &ExpressionView, name: &str) -> bool {
    if atom_text(binding)
        .is_some_and(|binding_name| common_lisp_symbol_reference_eq(binding_name, name))
    {
        return true;
    }

    binding
        .children
        .first()
        .is_some_and(|pattern| pattern_contains_name(pattern, name))
}

pub(super) fn iteration_binding_child_shadowed(
    binding_form: &ExpressionView,
    name: &str,
    child_index: usize,
) -> bool {
    child_index >= 2 && iteration_binding_binds_name(binding_form, name)
}

pub(super) fn local_callable_binding_child_shadowed(
    binding: &ExpressionView,
    name: &str,
    child_index: usize,
) -> bool {
    child_index >= 2
        && binding
            .children
            .get(1)
            .is_some_and(|parameters| lambda_list_contains_name(parameters, name))
}

fn binding_pairs_contain_name(bindings: &ExpressionView, name: &str) -> bool {
    if bindings.delimiter == Some(Delimiter::Bracket) {
        return bindings
            .children
            .iter()
            .step_by(2)
            .any(|binding| pattern_contains_name(binding, name));
    }

    bindings.children.iter().any(|binding| {
        binding
            .children
            .first()
            .is_some_and(|pattern| pattern_contains_name(pattern, name))
    })
}

fn clause_binds_name(clause: &ExpressionView, name: &str) -> bool {
    clause
        .children
        .get(1)
        .is_some_and(|parameters| lambda_list_contains_name(parameters, name))
}

fn iteration_binding_binds_name(binding_form: &ExpressionView, name: &str) -> bool {
    binding_form
        .children
        .first()
        .and_then(atom_text)
        .is_some_and(|binding_name| common_lisp_symbol_reference_eq(binding_name, name))
}

fn variable_specs_bind_name(bindings: &ExpressionView, name: &str) -> bool {
    bindings
        .children
        .iter()
        .any(|binding| variable_spec_binds_name(binding, name))
}

fn slot_specs_bind_name(slot_specs: &ExpressionView, name: &str) -> bool {
    slot_specs.children.iter().any(|slot_spec| {
        atom_text(slot_spec)
            .or_else(|| slot_spec.children.first().and_then(atom_text))
            .is_some_and(|binding_name| common_lisp_symbol_reference_eq(binding_name, name))
    })
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LambdaListMode {
    Required,
    Optional,
    Key,
    Aux,
}

fn lambda_list_contains_name(parameter_form: &ExpressionView, name: &str) -> bool {
    let mut mode = LambdaListMode::Required;
    let mut index = 0usize;

    while index < parameter_form.children.len() {
        let child = &parameter_form.children[index];
        if let Some(marker) = atom_text(child) {
            match marker {
                "&optional" => {
                    mode = LambdaListMode::Optional;
                    index += 1;
                    continue;
                }
                "&key" => {
                    mode = LambdaListMode::Key;
                    index += 1;
                    continue;
                }
                "&aux" => {
                    mode = LambdaListMode::Aux;
                    index += 1;
                    continue;
                }
                "&rest" | "&body" | "&whole" | "&environment" => {
                    if parameter_form
                        .children
                        .get(index + 1)
                        .is_some_and(|next| pattern_contains_name(next, name))
                    {
                        return true;
                    }
                    index += 2;
                    continue;
                }
                "&allow-other-keys" => {
                    index += 1;
                    continue;
                }
                _ if marker.starts_with('&') => {
                    index += 1;
                    continue;
                }
                _ => {}
            }
        }

        if lambda_list_parameter_spec_contains_name(child, mode, name) {
            return true;
        }
        index += 1;
    }

    false
}

fn lambda_list_parameter_spec_contains_name(
    spec: &ExpressionView,
    mode: LambdaListMode,
    name: &str,
) -> bool {
    if atom_text(spec).is_some() || mode == LambdaListMode::Required {
        return pattern_contains_name(spec, name);
    }

    match mode {
        LambdaListMode::Required => pattern_contains_name(spec, name),
        LambdaListMode::Optional => {
            spec.children
                .first()
                .is_some_and(|parameter| pattern_contains_name(parameter, name))
                || supplied_p_contains_name(spec, name)
        }
        LambdaListMode::Key => {
            spec.children
                .first()
                .is_some_and(|parameter| key_parameter_contains_name(parameter, name))
                || supplied_p_contains_name(spec, name)
        }
        LambdaListMode::Aux => spec
            .children
            .first()
            .is_some_and(|parameter| pattern_contains_name(parameter, name)),
    }
}

fn key_parameter_contains_name(spec_name: &ExpressionView, name: &str) -> bool {
    if spec_name.children.len() >= 2
        && atom_text(&spec_name.children[0]).is_some_and(|designator| designator.starts_with(':'))
    {
        return pattern_contains_name(&spec_name.children[1], name);
    }

    pattern_contains_name(spec_name, name)
}

fn supplied_p_contains_name(spec: &ExpressionView, name: &str) -> bool {
    spec.children
        .get(2)
        .is_some_and(|supplied_p| pattern_contains_name(supplied_p, name))
}

fn pattern_contains_name(view: &ExpressionView, name: &str) -> bool {
    atom_text(view)
        .map(|text| common_lisp_symbol_reference_eq(text, name))
        .unwrap_or_else(|| {
            view.children
                .iter()
                .any(|child| pattern_contains_name(child, name))
        })
}

fn list_head(view: &ExpressionView) -> Option<&str> {
    view.children.first().and_then(atom_text)
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    view.text.as_deref()
}
