use crate::domain::common_lisp::{common_lisp_symbol_name_eq, common_lisp_symbol_reference_eq};
use crate::domain::function_parameter::list_edit::list_head;
use crate::domain::sexpr::ExpressionView;
use crate::domain::sexpr::SymbolName;

pub(super) fn matched_setf_place_call<'a>(
    view: &'a ExpressionView,
    function_name: &SymbolName,
) -> Option<&'a ExpressionView> {
    let place = view.children.get(1)?;
    (list_head(view).is_some_and(|head| common_lisp_symbol_name_eq(head, "setf"))
        && list_head(place)
            .is_some_and(|head| common_lisp_symbol_reference_eq(head, function_name.as_str())))
    .then_some(place)
}
