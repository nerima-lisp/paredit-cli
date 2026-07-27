use crate::definition::macro_expander_body_range;
use crate::dialect::Dialect;
use crate::sexpr::{ExpressionKind, ExpressionView, Path, SelectionError, SexprResult, SyntaxTree};

use super::CommonLispLocalCallableForm;
use super::common_lisp_operator_head_eq;
use super::common_lisp_symbol_reference_eq;

#[must_use]
pub fn common_lisp_local_callable_form(
    dialect: Dialect,
    head: &str,
) -> Option<CommonLispLocalCallableForm> {
    dialect.common_lisp_local_callable_form_for_head(head)
}

/// Macro expander templates are syntax templates for executable output, so
/// quasiquoted forms in global and local macro expander bodies remain eligible
/// for refactoring.
pub fn common_lisp_macro_expander_path(
    tree: &SyntaxTree,
    dialect: Dialect,
    path: &Path,
) -> SexprResult<bool> {
    let indexes = path.to_raw_indexes();

    for ancestor_end in 1..indexes.len() {
        let descendant_indexes = &indexes[ancestor_end..];
        let ancestor = Path::from_indexes(indexes[..ancestor_end].to_vec());
        let view = tree.select_path(&ancestor)?.view();
        let Some(head) = atom_child(&view, 0) else {
            continue;
        };

        if descendant_indexes.first().is_some_and(|child_index| {
            macro_expander_body_range(dialect, &view, head)
                .is_some_and(|body_range| body_range.contains_child(*child_index))
        }) {
            return Ok(true);
        }

        if descendant_indexes.len() < 3 || descendant_indexes[0] != 1 || descendant_indexes[2] < 2 {
            continue;
        }

        if common_lisp_local_callable_form(dialect, head).is_some_and(|form| form.is_macro()) {
            return Ok(true);
        }
    }

    Ok(false)
}

#[must_use]
pub const fn is_macro_callable_form(form: CommonLispLocalCallableForm) -> bool {
    form.is_macro()
}

pub fn local_callable_names(view: &ExpressionView) -> Vec<String> {
    view.children
        .get(1)
        .into_iter()
        .flat_map(|bindings| bindings.children.iter())
        .filter_map(local_callable_name)
        .map(ToOwned::to_owned)
        .collect()
}

#[must_use]
pub fn is_local_callable_bound(scope: &[String], head: &str) -> bool {
    scope
        .iter()
        .any(|name| common_lisp_symbol_reference_eq(name, head))
}

#[must_use]
pub fn local_callable_body_scope(local_callables: &[String], view: &ExpressionView) -> Vec<String> {
    let mut body_scope = local_callables.to_vec();
    body_scope.extend(local_callable_names(view));
    body_scope
}

#[must_use]
pub const fn local_callable_binding_body_scope<'a>(
    form: CommonLispLocalCallableForm,
    local_callables: &'a [String],
    body_scope: &'a [String],
) -> &'a [String] {
    match form {
        CommonLispLocalCallableForm::Labels => body_scope,
        CommonLispLocalCallableForm::Flet
        | CommonLispLocalCallableForm::Macrolet
        | CommonLispLocalCallableForm::CompilerMacrolet => local_callables,
    }
}

#[must_use]
pub const fn local_callable_definition_reference_scope<'a>(
    form: CommonLispLocalCallableForm,
    local_callables: &'a [String],
    body_scope: &'a [String],
) -> &'a [String] {
    match form {
        CommonLispLocalCallableForm::Labels => local_callables,
        CommonLispLocalCallableForm::Flet
        | CommonLispLocalCallableForm::Macrolet
        | CommonLispLocalCallableForm::CompilerMacrolet => body_scope,
    }
}

pub fn local_callable_scope_at_path(
    tree: &SyntaxTree,
    dialect: Dialect,
    path: &Path,
) -> SexprResult<Vec<String>> {
    let indexes = path.to_raw_indexes();
    let Some((&root_index, descendants)) = indexes.split_first() else {
        return Ok(Vec::new());
    };
    let root_path = Path::root_child(root_index);
    let view = tree.select_path(&root_path)?.view();
    local_callable_scope_in_view(&view, dialect, descendants, &[]).ok_or_else(|| {
        SelectionError::PathNotReachable {
            path: path.to_string(),
        }
        .into()
    })
}

fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

fn local_callable_name(binding: &ExpressionView) -> Option<&str> {
    if binding.kind != ExpressionKind::List {
        return None;
    }
    if let Some(name) = atom_child(binding, 0) {
        return Some(name);
    }

    let name = binding.children.first()?;
    if name.kind != ExpressionKind::List {
        return None;
    }

    let head = atom_child(name, 0)?;
    if !common_lisp_operator_head_eq(head, "setf") {
        return None;
    }

    atom_child(name, 1)
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

fn local_callable_scope_in_view(
    view: &ExpressionView,
    dialect: Dialect,
    remaining_path: &[usize],
    current_scope: &[String],
) -> Option<Vec<String>> {
    if remaining_path.is_empty() {
        return Some(current_scope.to_vec());
    }

    let child_index = *remaining_path.first()?;
    let next_scope = if let Some(head) = atom_child(view, 0) {
        match common_lisp_local_callable_form(dialect, head) {
            Some(form) => scope_for_local_callable_child(
                view,
                form,
                child_index,
                current_scope,
                remaining_path,
            )?,
            None => current_scope.to_vec(),
        }
    } else {
        current_scope.to_vec()
    };

    let child = view.children.get(child_index)?;
    local_callable_scope_in_view(child, dialect, &remaining_path[1..], &next_scope)
}

fn scope_for_local_callable_child(
    view: &ExpressionView,
    form: CommonLispLocalCallableForm,
    child_index: usize,
    current_scope: &[String],
    remaining_path: &[usize],
) -> Option<Vec<String>> {
    if child_index >= 2 {
        return Some(local_callable_body_scope(current_scope, view));
    }

    if child_index != 1 {
        return Some(current_scope.to_vec());
    }

    let binding_index = *remaining_path.get(1)?;
    let binding_child_index = *remaining_path.get(2)?;
    view.children.get(1)?.children.get(binding_index)?;
    if binding_child_index < 2 {
        return Some(current_scope.to_vec());
    }

    let body_scope = local_callable_body_scope(current_scope, view);
    Some(local_callable_binding_body_scope(form, current_scope, &body_scope).to_vec())
}
