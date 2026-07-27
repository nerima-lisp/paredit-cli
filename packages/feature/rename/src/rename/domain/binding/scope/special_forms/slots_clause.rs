use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, SymbolName};

use crate::domain::rename::selection::atom_text;

use super::super::super::forms::parameter_form_binds;
use super::super::collect_symbol_atom_spans_unshadowed;

pub(super) fn collect_slot_binding_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    let Some(slot_specs) = view.children.get(1) else {
        return;
    };
    let Some(instance_form) = view.children.get(2) else {
        return;
    };

    collect_symbol_atom_spans_unshadowed(
        instance_form,
        symbol,
        output,
        shadowed_scope_count,
        input,
    );

    if slot_specs
        .children
        .iter()
        .any(|spec| slot_spec_binds(spec, symbol))
    {
        *shadowed_scope_count += 1;
        return;
    }

    for body in &view.children[3..] {
        collect_symbol_atom_spans_unshadowed(body, symbol, output, shadowed_scope_count, input);
    }
}

pub(super) fn collect_clause_form_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    if let Some(protected_form) = view.children.get(1) {
        collect_symbol_atom_spans_unshadowed(
            protected_form,
            symbol,
            output,
            shadowed_scope_count,
            input,
        );
    }

    for clause in &view.children[2..] {
        if clause.kind != ExpressionKind::List || clause.delimiter != Some(Delimiter::Paren) {
            collect_symbol_atom_spans_unshadowed(
                clause,
                symbol,
                output,
                shadowed_scope_count,
                input,
            );
            continue;
        }

        let Some(parameter_form) = clause.children.get(1) else {
            collect_symbol_atom_spans_unshadowed(
                clause,
                symbol,
                output,
                shadowed_scope_count,
                input,
            );
            continue;
        };

        if parameter_form_binds(parameter_form, symbol, input) {
            *shadowed_scope_count += 1;
            continue;
        }

        for body in &clause.children[2..] {
            collect_symbol_atom_spans_unshadowed(body, symbol, output, shadowed_scope_count, input);
        }
    }
}

fn slot_spec_binds(slot_spec: &ExpressionView, symbol: &SymbolName) -> bool {
    atom_text(slot_spec)
        .or_else(|| slot_spec.children.first().and_then(atom_text))
        .is_some_and(|name| common_lisp_symbol_reference_eq(name, symbol.as_str()))
}
