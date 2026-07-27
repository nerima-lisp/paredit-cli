use crate::domain::definition::{DefinitionCategory, definition_shape};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, SymbolName};

use super::super::super::collect_enclosing_lambda_list_references;
use super::super::collect_symbol_atom_spans_unshadowed;

pub(super) fn collect_defmethod_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    let Some(shape) = definition_shape(Dialect::CommonLisp, view, "defmethod")
        .filter(|shape| shape.category == DefinitionCategory::Method)
    else {
        return;
    };
    let Some(parameter_form) = shape.lambda_list(view) else {
        return;
    };

    if collect_enclosing_lambda_list_references(
        parameter_form,
        symbol,
        input,
        output,
        shadowed_scope_count,
    ) {
        return;
    }

    for body in shape.body_forms(view) {
        collect_symbol_atom_spans_unshadowed(body, symbol, output, shadowed_scope_count, input);
    }
}

pub(super) fn collect_handler_bind_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
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
            collect_symbol_atom_spans_unshadowed(
                function_form,
                symbol,
                output,
                shadowed_scope_count,
                input,
            );
        }

        if include_restart_options {
            collect_restart_bind_option_value_references(
                spec,
                symbol,
                output,
                shadowed_scope_count,
                input,
            );
        }
    }

    for body in &view.children[2..] {
        collect_symbol_atom_spans_unshadowed(body, symbol, output, shadowed_scope_count, input);
    }
}

pub(super) fn collect_local_callable_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    if let Some(binding_form) = view.children.get(1) {
        for binding in &binding_form.children {
            if binding.kind != ExpressionKind::List || binding.delimiter != Some(Delimiter::Paren) {
                continue;
            }

            let Some(parameter_form) = binding.children.get(1) else {
                continue;
            };

            if collect_enclosing_lambda_list_references(
                parameter_form,
                symbol,
                input,
                output,
                shadowed_scope_count,
            ) {
                continue;
            }

            for body in &binding.children[2..] {
                collect_symbol_atom_spans_unshadowed(
                    body,
                    symbol,
                    output,
                    shadowed_scope_count,
                    input,
                );
            }
        }
    }

    for body in &view.children[2..] {
        collect_symbol_atom_spans_unshadowed(body, symbol, output, shadowed_scope_count, input);
    }
}

fn collect_restart_bind_option_value_references(
    spec: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    let mut index = 2;
    while index + 1 < spec.children.len() {
        collect_symbol_atom_spans_unshadowed(
            &spec.children[index + 1],
            symbol,
            output,
            shadowed_scope_count,
            input,
        );
        index += 2;
    }
}
