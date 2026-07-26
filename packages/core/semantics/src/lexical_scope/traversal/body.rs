use crate::domain::common_lisp::is_common_lisp_declaration_form;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::collect_unshadowed_symbol_references_in_context;

pub(super) fn collect_body_forms(
    dialect: Dialect,
    body_forms: &[ExpressionView],
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let mut body_started = false;

    for body in body_forms {
        if !body_started
            && body
                .children
                .first()
                .and_then(super::super::syntax::atom_text)
                .is_some_and(is_common_lisp_declaration_form)
        {
            continue;
        }

        body_started = true;
        collect_unshadowed_symbol_references_in_context(dialect, body, symbol, input, output, 0);
    }
}
