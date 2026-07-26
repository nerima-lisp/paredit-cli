use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, SymbolName};

use super::super::body::collect_body_forms;
use super::super::collect_unshadowed_symbol_references_in_context;
use crate::domain::lexical_scope::bindings::{binding_binds, generic_binding_groups};

pub(super) fn collect_parallel_let_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };
    if binding_form.delimiter == Some(Delimiter::Bracket) {
        collect_sequential_let_references(dialect, view, symbol, input, output);
        return;
    }
    let Ok(bindings) = generic_binding_groups(binding_form) else {
        return;
    };

    for binding in &bindings {
        if let Some(value) = &binding.value {
            collect_unshadowed_symbol_references_in_context(
                dialect, value, symbol, input, output, 0,
            );
        }
    }

    if bindings
        .iter()
        .any(|binding| binding_binds(binding, symbol))
    {
        return;
    }

    collect_body_forms(dialect, &view.children[2..], symbol, input, output);
}

pub(super) fn collect_sequential_let_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };
    let Ok(bindings) = generic_binding_groups(binding_form) else {
        return;
    };

    for binding in &bindings {
        if let Some(value) = &binding.value {
            collect_unshadowed_symbol_references_in_context(
                dialect, value, symbol, input, output, 0,
            );
        }
        if binding_binds(binding, symbol) {
            return;
        }
    }

    collect_body_forms(dialect, &view.children[2..], symbol, input, output);
}

pub(super) fn collect_iteration_binding_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };

    if let Some(source_form) = binding_form.children.get(1) {
        collect_unshadowed_symbol_references_in_context(
            dialect,
            source_form,
            symbol,
            input,
            output,
            0,
        );
    }

    if iteration_binding_form_binds(binding_form, symbol) {
        return;
    }

    if let Some(result_form) = binding_form.children.get(2) {
        collect_unshadowed_symbol_references_in_context(
            dialect,
            result_form,
            symbol,
            input,
            output,
            0,
        );
    }

    collect_body_forms(dialect, &view.children[2..], symbol, input, output);
}

pub(super) fn collect_do_like_binding_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    sequential_scope: bool,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };

    if sequential_scope {
        for spec in &binding_form.children {
            if let Some(init_form) = variable_spec_init_form(spec) {
                collect_unshadowed_symbol_references_in_context(
                    dialect, init_form, symbol, input, output, 0,
                );
            }
            if variable_spec_binds(spec, symbol) {
                return;
            }
        }
    } else {
        for spec in &binding_form.children {
            if let Some(init_form) = variable_spec_init_form(spec) {
                collect_unshadowed_symbol_references_in_context(
                    dialect, init_form, symbol, input, output, 0,
                );
            }
        }
        if binding_form
            .children
            .iter()
            .any(|spec| variable_spec_binds(spec, symbol))
        {
            return;
        }
    }

    for spec in &binding_form.children {
        if let Some(step_form) = do_variable_spec_step_form(spec) {
            collect_unshadowed_symbol_references_in_context(
                dialect, step_form, symbol, input, output, 0,
            );
        }
    }

    collect_body_forms(dialect, &view.children[2..], symbol, input, output);
}

fn iteration_binding_form_binds(binding_form: &ExpressionView, symbol: &SymbolName) -> bool {
    binding_form
        .children
        .first()
        .and_then(super::super::super::syntax::atom_text)
        .is_some_and(|name| common_lisp_symbol_reference_eq(name, symbol.as_str()))
}

fn variable_spec_binds(spec: &ExpressionView, symbol: &SymbolName) -> bool {
    super::super::super::syntax::atom_text(spec)
        .or_else(|| {
            spec.children
                .first()
                .and_then(super::super::super::syntax::atom_text)
        })
        .is_some_and(|name| common_lisp_symbol_reference_eq(name, symbol.as_str()))
}

fn variable_spec_init_form(spec: &ExpressionView) -> Option<&ExpressionView> {
    (spec.kind == ExpressionKind::List)
        .then(|| spec.children.get(1))
        .flatten()
}

fn do_variable_spec_step_form(spec: &ExpressionView) -> Option<&ExpressionView> {
    (spec.kind == ExpressionKind::List)
        .then(|| spec.children.get(2))
        .flatten()
}
