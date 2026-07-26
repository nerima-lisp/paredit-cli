use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, SymbolName};

use super::super::body::collect_body_forms;
use super::super::collect_unshadowed_symbol_references_in_context;
use crate::domain::lexical_scope::bindings::parameter_form_binds;

pub(super) fn collect_clause_binding_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let Some(protected_form) = view.children.get(1) else {
        return;
    };

    collect_unshadowed_symbol_references_in_context(
        dialect,
        protected_form,
        symbol,
        input,
        output,
        0,
    );

    for clause in &view.children[2..] {
        collect_clause_body_references(dialect, clause, symbol, input, output);
    }
}

pub(super) fn collect_handler_bind_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    include_restart_options: bool,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };

    for spec in &binding_form.children {
        if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
            continue;
        }

        if let Some(function_form) = spec.children.get(1) {
            collect_unshadowed_symbol_references_in_context(
                dialect,
                function_form,
                symbol,
                input,
                output,
                0,
            );
        }

        if include_restart_options {
            collect_restart_bind_option_value_references(dialect, spec, symbol, input, output);
        }
    }

    collect_body_forms(dialect, &view.children[2..], symbol, input, output);
}

fn collect_clause_body_references(
    dialect: Dialect,
    clause: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    if clause.kind != ExpressionKind::List {
        collect_unshadowed_symbol_references_in_context(dialect, clause, symbol, input, output, 0);
        return;
    }

    let Some(parameter_form) = clause.children.get(1) else {
        return;
    };

    if parameter_form_binds(parameter_form, symbol) {
        return;
    }

    collect_body_forms(dialect, &clause.children[2..], symbol, input, output);
}

fn collect_restart_bind_option_value_references(
    dialect: Dialect,
    spec: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let mut index = 2;
    while index + 1 < spec.children.len() {
        collect_unshadowed_symbol_references_in_context(
            dialect,
            &spec.children[index + 1],
            symbol,
            input,
            output,
            0,
        );
        index += 2;
    }
}
