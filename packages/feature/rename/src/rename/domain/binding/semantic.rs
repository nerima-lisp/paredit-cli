use crate::error::{RenameResult, SemanticShapeError};

use paredit_core_syntax::dialect::{
    BinderShape, BindingVisibility, BodyShape, DefinitionShape, ParameterShape, RelativeNodePath,
    RenameBindingOperation, ScopeShape, VerifiedSemanticPolicy,
};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SymbolName};

use super::build_binding_rename_parts;
use super::destructure::binding_pattern_name_spans;
use super::forms::{binding_groups, parameter_name_spans};
use super::types::{BindingGroup, BindingRenameParts, ParameterNameSpan};

pub fn semantic_binding_rename_parts(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    from: &SymbolName,
    form: String,
    input: &str,
) -> RenameResult<BindingRenameParts> {
    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0;

    let binding = if let Some(scope) = semantic.scope_shape(view) {
        select_scope_binding_and_collect(
            semantic,
            view,
            scope,
            from,
            input,
            &mut reference_spans,
            &mut shadowed_scope_count,
        )?
    } else if let Some(definition) = semantic.definition_shape(view) {
        select_definition_binding_and_collect(
            semantic,
            view,
            definition,
            from,
            input,
            &mut reference_spans,
            &mut shadowed_scope_count,
        )?
    } else {
        return Err(SemanticShapeError::NoVerifiedShape.into());
    };

    Ok(build_binding_rename_parts(
        form,
        view.span,
        binding.name_span,
        binding.binding_edit,
        reference_spans,
        shadowed_scope_count,
    ))
}

fn select_scope_binding_and_collect(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    scope: ScopeShape,
    from: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
) -> RenameResult<ParameterNameSpan> {
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
        } => {
            let groups = binding_groups(
                semantic.dialect(),
                resolve_relative(view, container)
                    .ok_or(SemanticShapeError::BindingContainerMissing)?,
                input,
            )?;
            let (binding, group_index) = select_group_binding(semantic, &groups, from)?;
            collect_selected_binding_scope(
                semantic,
                view,
                &groups,
                group_index,
                visibility,
                scope.body(),
                from,
                input,
                output,
                shadowed_scope_count,
            );
            Ok(binding)
        }
        BinderShape::NamedBindingList {
            scope_name,
            container,
            visibility,
            ..
        } => {
            let name = single_pattern_binding(
                resolve_relative(view, scope_name)
                    .ok_or(SemanticShapeError::NamedScopeNameMissing)?,
                input,
            )?;
            let groups = binding_groups(
                semantic.dialect(),
                resolve_relative(view, container)
                    .ok_or(SemanticShapeError::BindingContainerMissing)?,
                input,
            )?;
            let mut candidates = matching_group_bindings(semantic, &groups, from);
            if identifiers_equal(semantic, &name.name, from) {
                candidates.push((name.clone(), None));
            }
            let (binding, group_index) = select_unique_indexed(candidates)?;

            if let Some(group_index) = group_index {
                collect_selected_binding_scope(
                    semantic,
                    view,
                    &groups,
                    group_index,
                    visibility,
                    scope.body(),
                    from,
                    input,
                    output,
                    shadowed_scope_count,
                );
            } else {
                collect_body_references(
                    semantic,
                    view,
                    scope.body(),
                    from,
                    input,
                    output,
                    shadowed_scope_count,
                );
            }
            Ok(binding)
        }
        BinderShape::Parameters(parameters) => {
            let bindings = parameter_bindings(view, parameters, input)?;
            let binding = select_unique_binding(semantic, bindings, from)?;
            collect_body_references(
                semantic,
                view,
                scope.body(),
                from,
                input,
                output,
                shadowed_scope_count,
            );
            Ok(binding)
        }
        BinderShape::NamedParameters { name, parameters } => {
            let mut bindings = parameter_bindings(view, parameters, input)?;
            bindings.push(single_pattern_binding(
                resolve_relative(view, name).ok_or(SemanticShapeError::NamedCallableNameMissing)?,
                input,
            )?);
            let binding = select_unique_binding(semantic, bindings, from)?;
            collect_body_references(
                semantic,
                view,
                scope.body(),
                from,
                input,
                output,
                shadowed_scope_count,
            );
            Ok(binding)
        }
        BinderShape::ParameterClauses {
            name,
            first_clause_index,
            parameters,
        } => {
            let local_name = name
                .map(|path| {
                    resolve_relative(view, path)
                        .ok_or_else(|| SemanticShapeError::NamedCallableNameMissing.into())
                        .and_then(|name| single_pattern_binding(name, input))
                })
                .transpose()?;
            let mut candidates = Vec::new();
            if let Some(name) = &local_name {
                if identifiers_equal(semantic, &name.name, from) {
                    candidates.push((name.clone(), None));
                }
            }
            for (clause_index, clause) in view.children.iter().enumerate().skip(first_clause_index)
            {
                for binding in parameter_bindings(clause, parameters, input)? {
                    if identifiers_equal(semantic, &binding.name, from) {
                        candidates.push((binding, Some(clause_index)));
                    }
                }
            }
            let (binding, clause_index) = select_unique_indexed(candidates)?;
            match clause_index {
                None => collect_body_references(
                    semantic,
                    view,
                    scope.body(),
                    from,
                    input,
                    output,
                    shadowed_scope_count,
                ),
                Some(clause_index) => collect_clause_body_references(
                    semantic,
                    view,
                    scope.body(),
                    clause_index,
                    from,
                    input,
                    output,
                    shadowed_scope_count,
                )?,
            }
            Ok(binding)
        }
    }
}

fn select_definition_binding_and_collect(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    definition: DefinitionShape,
    from: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
) -> RenameResult<ParameterNameSpan> {
    let parameters = definition
        .parameters()
        .ok_or(SemanticShapeError::NoLexicalParameters)?;
    let binding =
        select_unique_binding(semantic, parameter_bindings(view, parameters, input)?, from)?;
    collect_body_references(
        semantic,
        view,
        definition.body(),
        from,
        input,
        output,
        shadowed_scope_count,
    );
    Ok(binding)
}

#[allow(clippy::too_many_arguments)]
fn collect_selected_binding_scope(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    groups: &[BindingGroup],
    selected_group: usize,
    visibility: BindingVisibility,
    body: BodyShape,
    from: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
) {
    if visibility == BindingVisibility::Sequential {
        for group in groups.iter().skip(selected_group + 1) {
            if let Some(value) = &group.value {
                collect_references(
                    semantic,
                    value,
                    from,
                    input,
                    output,
                    shadowed_scope_count,
                    false,
                );
            }
        }
    }
    collect_body_references(
        semantic,
        view,
        body,
        from,
        input,
        output,
        shadowed_scope_count,
    );
}

fn select_group_binding(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    groups: &[BindingGroup],
    from: &SymbolName,
) -> RenameResult<(ParameterNameSpan, usize)> {
    let candidates = matching_group_bindings(semantic, groups, from)
        .into_iter()
        .map(|(binding, index)| {
            Ok((
                binding,
                index.ok_or(SemanticShapeError::BindingGroupIndexMissing)?,
            ))
        })
        .collect::<RenameResult<Vec<_>>>()?;
    let mut candidates = candidates.into_iter();
    let candidate = candidates
        .next()
        .ok_or(SemanticShapeError::BindingNameNotFound)?;
    if candidates.next().is_some() {
        return Err(SemanticShapeError::BindingNameAmbiguous.into());
    }
    Ok(candidate)
}

fn matching_group_bindings(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    groups: &[BindingGroup],
    from: &SymbolName,
) -> Vec<(ParameterNameSpan, Option<usize>)> {
    groups
        .iter()
        .enumerate()
        .flat_map(|(index, group)| {
            group
                .names
                .iter()
                .filter(move |binding| identifiers_equal(semantic, &binding.name, from))
                .cloned()
                .map(move |binding| (binding, Some(index)))
        })
        .collect()
}

fn select_unique_binding(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    bindings: Vec<ParameterNameSpan>,
    from: &SymbolName,
) -> RenameResult<ParameterNameSpan> {
    let mut matches = bindings
        .into_iter()
        .filter(|binding| identifiers_equal(semantic, &binding.name, from));
    let binding = matches
        .next()
        .ok_or(SemanticShapeError::BindingNameNotFound)?;
    if matches.next().is_some() {
        return Err(SemanticShapeError::BindingNameAmbiguous.into());
    }
    Ok(binding)
}

fn select_unique_indexed(
    candidates: Vec<(ParameterNameSpan, Option<usize>)>,
) -> RenameResult<(ParameterNameSpan, Option<usize>)> {
    let mut candidates = candidates.into_iter();
    let candidate = candidates
        .next()
        .ok_or(SemanticShapeError::BindingNameNotFound)?;
    if candidates.next().is_some() {
        return Err(SemanticShapeError::BindingNameAmbiguous.into());
    }
    Ok(candidate)
}

fn parameter_bindings(
    view: &ExpressionView,
    parameters: ParameterShape,
    input: &str,
) -> RenameResult<Vec<ParameterNameSpan>> {
    let mut container = resolve_relative(view, parameters.container())
        .ok_or(SemanticShapeError::ParameterContainerMissing)?
        .clone();
    let first = parameters.first_parameter_index();
    if first > container.children.len() {
        return Err(SemanticShapeError::ParameterLayoutOutsideContainer.into());
    }
    container.children.drain(..first);
    parameter_name_spans(&container, input)
}

fn single_pattern_binding(view: &ExpressionView, input: &str) -> RenameResult<ParameterNameSpan> {
    let mut bindings = binding_pattern_name_spans(view, input).into_iter();
    let binding = bindings
        .next()
        .ok_or(SemanticShapeError::PatternHasNoName)?;
    if bindings.next().is_some() {
        return Err(SemanticShapeError::PatternNotOneName.into());
    }
    Ok(binding)
}

fn collect_references(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    from: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    is_call_head: bool,
) {
    if view.kind == ExpressionKind::Atom {
        if !is_lisp2_call_head(semantic, is_call_head)
            && view
                .text
                .as_deref()
                .is_some_and(|name| semantic.identifiers_equal(name, from.as_str()))
        {
            output.push(view.span);
        }
        return;
    }

    if let Some(scope) = semantic.scope_shape(view) {
        collect_nested_scope_references(
            semantic,
            view,
            scope,
            from,
            input,
            output,
            shadowed_scope_count,
        );
        return;
    }
    if let Some(definition) = semantic.definition_shape(view) {
        collect_nested_definition_references(
            semantic,
            view,
            definition,
            from,
            input,
            output,
            shadowed_scope_count,
        );
        return;
    }
    if semantic.dialect() == paredit_core_syntax::dialect::Dialect::EmacsLisp
        && super::collect_shadow_aware_special_form(view, from, output, shadowed_scope_count, input)
    {
        return;
    }

    for (index, child) in view.children.iter().enumerate() {
        collect_references(
            semantic,
            child,
            from,
            input,
            output,
            shadowed_scope_count,
            index == 0,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_nested_scope_references(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    scope: ScopeShape,
    from: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
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
        } => {
            let Some(container) = resolve_relative(view, container) else {
                return;
            };
            let Ok(groups) = binding_groups(semantic.dialect(), container, input) else {
                return;
            };
            collect_nested_binding_groups(
                semantic,
                view,
                &groups,
                visibility,
                scope.body(),
                false,
                from,
                input,
                output,
                shadowed_scope_count,
            );
        }
        BinderShape::NamedBindingList {
            scope_name,
            container,
            visibility,
            ..
        } => {
            let Some(container) = resolve_relative(view, container) else {
                return;
            };
            let Ok(groups) = binding_groups(semantic.dialect(), container, input) else {
                return;
            };
            let shadows_from = resolve_relative(view, scope_name)
                .is_some_and(|name| pattern_binds(semantic, name, from, input));
            collect_nested_binding_groups(
                semantic,
                view,
                &groups,
                visibility,
                scope.body(),
                shadows_from,
                from,
                input,
                output,
                shadowed_scope_count,
            );
        }
        BinderShape::Parameters(parameters) => {
            let shadows_from = parameter_bindings(view, parameters, input)
                .is_ok_and(|bindings| bindings_bind(semantic, &bindings, from));
            collect_nested_parameter_body(
                semantic,
                view,
                scope.body(),
                shadows_from,
                from,
                input,
                output,
                shadowed_scope_count,
            );
        }
        BinderShape::NamedParameters { name, parameters } => {
            let shadows_from = resolve_relative(view, name)
                .is_some_and(|name| pattern_binds(semantic, name, from, input))
                || parameter_bindings(view, parameters, input)
                    .is_ok_and(|bindings| bindings_bind(semantic, &bindings, from));
            collect_nested_parameter_body(
                semantic,
                view,
                scope.body(),
                shadows_from,
                from,
                input,
                output,
                shadowed_scope_count,
            );
        }
        BinderShape::ParameterClauses {
            name,
            first_clause_index,
            parameters,
        } => {
            let name_shadows = name
                .and_then(|name| resolve_relative(view, name))
                .is_some_and(|name| pattern_binds(semantic, name, from, input));
            let BodyShape::ClauseChildrenFrom {
                body_child_index, ..
            } = scope.body()
            else {
                return;
            };
            let mut counted_name_shadow = false;
            for clause in view.children.iter().skip(first_clause_index) {
                let parameter_shadows = parameter_bindings(clause, parameters, input)
                    .is_ok_and(|bindings| bindings_bind(semantic, &bindings, from));
                if name_shadows || parameter_shadows {
                    if parameter_shadows || !counted_name_shadow {
                        *shadowed_scope_count += 1;
                    }
                    counted_name_shadow |= name_shadows;
                    continue;
                }
                for child in clause.children.iter().skip(body_child_index) {
                    collect_references(
                        semantic,
                        child,
                        from,
                        input,
                        output,
                        shadowed_scope_count,
                        false,
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_nested_binding_groups(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    groups: &[BindingGroup],
    visibility: BindingVisibility,
    body: BodyShape,
    name_shadows: bool,
    from: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
) {
    let mut binding_shadows = false;
    for group in groups {
        if visibility == BindingVisibility::Parallel || !binding_shadows {
            if let Some(value) = &group.value {
                collect_references(
                    semantic,
                    value,
                    from,
                    input,
                    output,
                    shadowed_scope_count,
                    false,
                );
            }
        }
        binding_shadows |= bindings_bind(semantic, &group.names, from);
    }

    if name_shadows || binding_shadows {
        *shadowed_scope_count += 1;
    } else {
        collect_body_references(
            semantic,
            view,
            body,
            from,
            input,
            output,
            shadowed_scope_count,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_nested_parameter_body(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    body: BodyShape,
    shadows_from: bool,
    from: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
) {
    if shadows_from {
        *shadowed_scope_count += 1;
    } else {
        collect_body_references(
            semantic,
            view,
            body,
            from,
            input,
            output,
            shadowed_scope_count,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_nested_definition_references(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    definition: DefinitionShape,
    from: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
) {
    let name_shadows = definition
        .name()
        .and_then(|name| resolve_relative(view, name))
        .is_some_and(|name| pattern_binds(semantic, name, from, input));
    let parameter_shadows = definition.parameters().is_some_and(|parameters| {
        parameter_bindings(view, parameters, input)
            .is_ok_and(|bindings| bindings_bind(semantic, &bindings, from))
    });
    if name_shadows || parameter_shadows {
        *shadowed_scope_count += 1;
    } else {
        collect_body_references(
            semantic,
            view,
            definition.body(),
            from,
            input,
            output,
            shadowed_scope_count,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_body_references(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    body: BodyShape,
    from: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
) {
    match body {
        BodyShape::ChildrenFrom(first) => {
            for child in view.children.iter().skip(first) {
                collect_references(
                    semantic,
                    child,
                    from,
                    input,
                    output,
                    shadowed_scope_count,
                    false,
                );
            }
        }
        BodyShape::ChildrenAfter(path) => {
            for child in view.children.iter().skip(path.child() + 1) {
                collect_references(
                    semantic,
                    child,
                    from,
                    input,
                    output,
                    shadowed_scope_count,
                    false,
                );
            }
        }
        BodyShape::ClauseChildrenFrom {
            first_clause_index,
            body_child_index,
        } => {
            for clause in view.children.iter().skip(first_clause_index) {
                for child in clause.children.iter().skip(body_child_index) {
                    collect_references(
                        semantic,
                        child,
                        from,
                        input,
                        output,
                        shadowed_scope_count,
                        false,
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_clause_body_references(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    body: BodyShape,
    clause_index: usize,
    from: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
) -> RenameResult<()> {
    let BodyShape::ClauseChildrenFrom {
        first_clause_index,
        body_child_index,
    } = body
    else {
        return Err(SemanticShapeError::ClauseBodyMetadataMissing.into());
    };
    if clause_index < first_clause_index {
        return Err(SemanticShapeError::ParameterOutsideClauses.into());
    }
    let clause = view
        .children
        .get(clause_index)
        .ok_or(SemanticShapeError::ClauseMissing)?;
    for child in clause.children.iter().skip(body_child_index) {
        collect_references(
            semantic,
            child,
            from,
            input,
            output,
            shadowed_scope_count,
            false,
        );
    }
    Ok(())
}

fn resolve_relative(view: &ExpressionView, path: RelativeNodePath) -> Option<&ExpressionView> {
    let child = view.children.get(path.child())?;
    path.grandchild()
        .map_or(Some(child), |grandchild| child.children.get(grandchild))
}

fn pattern_binds(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    pattern: &ExpressionView,
    from: &SymbolName,
    input: &str,
) -> bool {
    binding_pattern_name_spans(pattern, input)
        .iter()
        .any(|binding| identifiers_equal(semantic, &binding.name, from))
}

fn bindings_bind(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    bindings: &[ParameterNameSpan],
    from: &SymbolName,
) -> bool {
    bindings
        .iter()
        .any(|binding| identifiers_equal(semantic, &binding.name, from))
}

fn identifiers_equal(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    candidate: &str,
    from: &SymbolName,
) -> bool {
    semantic.identifiers_equal(candidate, from.as_str())
}

const fn is_lisp2_call_head(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    is_call_head: bool,
) -> bool {
    is_call_head
        && matches!(
            semantic.dialect(),
            paredit_core_syntax::dialect::Dialect::CommonLisp
                | paredit_core_syntax::dialect::Dialect::EmacsLisp
        )
}
