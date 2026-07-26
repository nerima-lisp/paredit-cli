use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::super::body::collect_body_forms;
use super::super::collect_unshadowed_symbol_references_in_context;
use crate::domain::lexical_scope::bindings::parameter_form_binds;

pub(super) fn collect_value_binding_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };
    let Some(value_form) = view.children.get(2) else {
        return;
    };

    collect_unshadowed_symbol_references_in_context(dialect, value_form, symbol, input, output, 0);

    if parameter_form_binds(binding_form, symbol) {
        return;
    }

    collect_body_forms(dialect, &view.children[3..], symbol, input, output);
}
