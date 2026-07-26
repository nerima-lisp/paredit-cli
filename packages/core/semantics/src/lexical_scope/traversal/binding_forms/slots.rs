use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::super::body::collect_body_forms;
use super::super::collect_unshadowed_symbol_references_in_context;

pub(super) fn collect_slot_binding_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let Some(slot_specs) = view.children.get(1) else {
        return;
    };
    let Some(instance_form) = view.children.get(2) else {
        return;
    };

    collect_unshadowed_symbol_references_in_context(
        dialect,
        instance_form,
        symbol,
        input,
        output,
        0,
    );

    if slot_specs
        .children
        .iter()
        .any(|spec| slot_spec_binds(spec, symbol))
    {
        return;
    }

    collect_body_forms(dialect, &view.children[3..], symbol, input, output);
}

fn slot_spec_binds(slot_spec: &ExpressionView, symbol: &SymbolName) -> bool {
    super::super::super::syntax::atom_text(slot_spec)
        .or_else(|| {
            slot_spec
                .children
                .first()
                .and_then(super::super::super::syntax::atom_text)
        })
        .is_some_and(|name| common_lisp_symbol_reference_eq(name, symbol.as_str()))
}
