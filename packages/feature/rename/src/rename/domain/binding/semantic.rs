use crate::error::{RenameResult, SemanticShapeError};

use paredit_core_syntax::dialect::{
    BinderShape, BindingVisibility, BodyShape, DefinitionShape, NameListArity, ParameterShape,
    RelativeNodePath, RenameBindingOperation, ScopeShape, VerifiedSemanticPolicy,
};
use paredit_core_syntax::sexpr::{
    ByteOffset, ByteSpan, ExpressionKind, ExpressionView, SymbolName,
};

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
        BinderShape::NameList {
            container,
            first_name_index,
            names,
        } => {
            let bindings = name_list_bindings(view, container, first_name_index, names, input)?;
            let binding = select_unique_binding(semantic, bindings, from)?;
            // The expressions that drive the iteration sit between the names
            // and the body but are evaluated outside this scope, so they are
            // not references to the binding being renamed.
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
        BinderShape::SingleName { name } => {
            let binding = single_pattern_binding(
                resolve_relative(view, name).ok_or(SemanticShapeError::BindingContainerMissing)?,
                input,
            )?;
            let binding = select_unique_binding(semantic, vec![binding], from)?;
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
    // Which of the group's own initializers can see the binding being renamed.
    //
    // A parallel `let` evaluates all of them outside the scope, so none. A
    // `let*` exposes it to the entries written after it. A `letrec` exposes it
    // to all of them, including its own -- `(letrec ((f (lambda () (f)))))` is
    // the whole point of the form, and skipping those occurrences would leave
    // a renamed procedure calling its old name.
    let visible_initializers: &[BindingGroup] = match visibility {
        BindingVisibility::Parallel => &[],
        BindingVisibility::Sequential => groups.get(selected_group + 1..).unwrap_or_default(),
        BindingVisibility::Recursive => groups,
    };

    for group in visible_initializers {
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

/// Resolves the bare names a `NameList` binder introduces, ignoring the
/// trailing children that drive the form rather than bind anything.
fn name_list_bindings(
    view: &ExpressionView,
    container: RelativeNodePath,
    first_name_index: usize,
    names: NameListArity,
    input: &str,
) -> RenameResult<Vec<ParameterNameSpan>> {
    let container =
        resolve_relative(view, container).ok_or(SemanticShapeError::BindingContainerMissing)?;
    let available = container
        .children
        .len()
        .checked_sub(first_name_index)
        .ok_or(SemanticShapeError::ParameterLayoutOutsideContainer)?;
    let name_count = names
        .name_count(available)
        .ok_or(SemanticShapeError::ParameterLayoutOutsideContainer)?;
    Ok(container
        .children
        .iter()
        .skip(first_name_index)
        .take(name_count)
        .flat_map(|name| binding_pattern_name_spans(name, input))
        .collect())
}

/// Walks the children of a `NameList` binder that drive the form. They are
/// evaluated in the enclosing scope, so references there are not shadowed.
#[allow(clippy::too_many_arguments)]
fn collect_name_list_drivers(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    container: RelativeNodePath,
    first_name_index: usize,
    names: NameListArity,
    from: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
) {
    let Some(container) = resolve_relative(view, container) else {
        return;
    };
    let Some(name_count) = container
        .children
        .len()
        .checked_sub(first_name_index)
        .and_then(|available| names.name_count(available))
    else {
        return;
    };
    for driver in container
        .children
        .iter()
        .skip(first_name_index + name_count)
    {
        collect_references(
            semantic,
            driver,
            from,
            input,
            output,
            shadowed_scope_count,
            false,
        );
    }
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
        if !is_lisp2_call_head(semantic, is_call_head) {
            if let Some(span) = reference_span(semantic, view, from) {
                output.push(span);
            }
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
        BinderShape::NameList {
            container,
            first_name_index,
            names,
        } => {
            // The driver expressions are evaluated in this enclosing scope, so
            // they still hold renameable references even when the names below
            // shadow the symbol for the body.
            collect_name_list_drivers(
                semantic,
                view,
                container,
                first_name_index,
                names,
                from,
                input,
                output,
                shadowed_scope_count,
            );
            let shadows_from = name_list_bindings(view, container, first_name_index, names, input)
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
        BinderShape::SingleName { name } => {
            let shadows_from = resolve_relative(view, name)
                .is_some_and(|name| pattern_binds(semantic, name, from, input));
            // Everything between the name and the body drives the iteration and
            // is evaluated outside the scope the name opens.
            let body_start = match scope.body() {
                BodyShape::ChildrenFrom(first) => first,
                BodyShape::ChildrenAfter(_) | BodyShape::ClauseChildrenFrom { .. } => return,
            };
            for (index, child) in view.children.iter().enumerate().take(body_start) {
                if index <= name.child() {
                    continue;
                }
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
    // A `letrec` binds its whole group before any initializer runs, so one
    // entry naming the symbol hides the outer binding from *every*
    // initializer -- including those written before it. `let` and `let*` are
    // decided entry by entry in the loop below.
    let group_shadows_every_initializer = visibility == BindingVisibility::Recursive
        && groups
            .iter()
            .any(|group| bindings_bind(semantic, &group.names, from));

    let mut binding_shadows = false;
    for group in groups {
        let initializer_sees_outer = !group_shadows_every_initializer
            && (visibility == BindingVisibility::Parallel || !binding_shadows);
        if initializer_sees_outer {
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

/// Returns the span to rewrite when `view` refers to `from`.
///
/// Usually that is the whole atom. In dialects where an atom can name a
/// binding and then reach into its value — Fennel's `handle:read`, Hy's
/// `obj.attr` — only the leading segment is the name, so rewriting the whole
/// atom would destroy the access path and rewriting nothing would leave the
/// reference pointing at a name that no longer exists.
fn reference_span(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    view: &ExpressionView,
    from: &SymbolName,
) -> Option<ByteSpan> {
    let text = view.text.as_deref()?;
    if identifiers_equal(semantic, text, from) {
        return Some(view.span);
    }

    let root_len = semantic.dialect().binding_reference_root_len(text)?;
    if !identifiers_equal(semantic, text.get(..root_len)?, from) {
        return None;
    }
    let start = view.span.start();
    ByteSpan::try_new(start, ByteOffset::new(start.get().checked_add(root_len)?))
}

const fn is_lisp2_call_head(
    semantic: VerifiedSemanticPolicy<RenameBindingOperation>,
    is_call_head: bool,
) -> bool {
    is_call_head && semantic.dialect().separates_function_namespace()
}
