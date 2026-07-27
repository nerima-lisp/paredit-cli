use crate::domain::common_lisp::{common_lisp_operator_head_eq, common_lisp_symbol_reference_eq};
use crate::domain::definition::{DefinitionCategory, definition_shape};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ExpressionKind, ExpressionView, SymbolName};

use super::super::selection::atom_text;

pub(super) fn is_target_define_symbol_macro(
    view: &ExpressionView,
    dialect: Dialect,
    from: &SymbolName,
) -> bool {
    let Some(head) = list_head(view) else {
        return false;
    };
    if !common_lisp_operator_head_eq(head, "define-symbol-macro") {
        return false;
    }
    definition_shape(dialect, view, head)
        .filter(|shape| shape.category == DefinitionCategory::Variable)
        .and_then(|shape| shape.name(view))
        .is_some_and(|name| common_lisp_symbol_reference_eq(name, from.as_str()))
}

pub(super) fn list_head(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::List)
        .then(|| view.children.first())
        .flatten()
        .and_then(atom_text)
}
