use crate::error::{CallSelectionError, FunctionParameterResult};

use crate::function_parameter::domain::list_edit::atom_text;
use paredit_core_syntax::common_lisp::{
    common_lisp_symbol_name_eq, common_lisp_symbol_reference_eq,
};
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView, SymbolName};

pub fn matches_function_call_view(view: &ExpressionView, function_name: &SymbolName) -> bool {
    direct_function_call_head(view)
        .is_some_and(|head| common_lisp_symbol_reference_eq(head, function_name.as_str()))
        || setf_place_call_head(view)
            .is_some_and(|head| common_lisp_symbol_reference_eq(head, function_name.as_str()))
}

pub fn ensure_matching_function_call(
    view: &ExpressionView,
    function_name: &SymbolName,
    command: &'static str,
) -> FunctionParameterResult<()> {
    if !matches_function_call_view(view, function_name) {
        if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
            return Err(CallSelectionError::SelectionNotACallList { command }.into());
        }
        let head = atom_text(
            view.children
                .first()
                .ok_or(CallSelectionError::CallEmpty { command })?,
        )
        .ok_or(CallSelectionError::CallHeadNotAnAtom { command })?;
        return Err(CallSelectionError::SelectionHeadMismatch {
            command,
            head: head.to_string(),
            function: function_name.to_string(),
        }
        .into());
    }

    Ok(())
}

pub struct FunctionCallView<'a> {
    pub view: &'a ExpressionView,
    pub argument_offset: usize,
}

pub fn resolve_function_call_view<'a>(
    view: &'a ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    command: &'static str,
) -> FunctionParameterResult<FunctionCallView<'a>> {
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
        .ok_or(CallSelectionError::SetfMissingPlace { command })?;
    if place.kind != ExpressionKind::List || place.delimiter != Some(Delimiter::Paren) {
        return Err(CallSelectionError::SetfPlaceNotACallList { command }.into());
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
