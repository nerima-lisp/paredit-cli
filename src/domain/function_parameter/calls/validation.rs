use anyhow::{Context, Result};

use crate::domain::common_lisp::{common_lisp_symbol_name_eq, common_lisp_symbol_reference_eq};
use crate::domain::function_parameter::list_edit::atom_text;
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView, SymbolName};

pub(in crate::domain::function_parameter) fn matches_function_call_view(
    view: &ExpressionView,
    function_name: &SymbolName,
) -> bool {
    direct_function_call_head(view)
        .is_some_and(|head| common_lisp_symbol_reference_eq(head, function_name.as_str()))
        || setf_place_call_head(view)
            .is_some_and(|head| common_lisp_symbol_reference_eq(head, function_name.as_str()))
}

pub(super) fn ensure_matching_function_call(
    view: &ExpressionView,
    function_name: &SymbolName,
    command: &str,
) -> Result<()> {
    if !matches_function_call_view(view, function_name) {
        if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
            anyhow::bail!("{command} call selection must be a function call list");
        }
        let head = atom_text(
            view.children
                .first()
                .with_context(|| format!("{command} call must not be empty"))?,
        )
        .with_context(|| format!("{command} call must start with an atom"))?;
        anyhow::bail!(
            "{command} call head '{head}' does not match selected definition '{function_name}'"
        );
    }

    Ok(())
}

pub(in crate::domain::function_parameter) struct FunctionCallView<'a> {
    pub(in crate::domain::function_parameter) view: &'a ExpressionView,
    pub(in crate::domain::function_parameter) argument_offset: usize,
}

pub(in crate::domain::function_parameter) fn resolve_function_call_view<'a>(
    view: &'a ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    command: &str,
) -> Result<FunctionCallView<'a>> {
    ensure_matching_function_call(view, function_name, command)?;

    if direct_function_call_head(view)
        .is_some_and(|head| common_lisp_symbol_reference_eq(head, function_name.as_str()))
    {
        return Ok(FunctionCallView {
            view,
            argument_offset: call_argument_offset,
        });
    }

    let place = view
        .children
        .get(1)
        .with_context(|| format!("{command} setf call must contain a place form"))?;
    if place.kind != ExpressionKind::List || place.delimiter != Some(Delimiter::Paren) {
        anyhow::bail!("{command} setf place must be a function call list");
    }
    Ok(FunctionCallView {
        view: place,
        argument_offset: 0,
    })
}

fn direct_function_call_head(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren))
        .then(|| view.children.first())
        .flatten()
        .and_then(atom_text)
}

fn setf_place_call_head(view: &ExpressionView) -> Option<&str> {
    if !direct_function_call_head(view).is_some_and(|head| common_lisp_symbol_name_eq(head, "setf"))
    {
        return None;
    }

    let place = view.children.get(1)?;
    direct_function_call_head(place)
}
