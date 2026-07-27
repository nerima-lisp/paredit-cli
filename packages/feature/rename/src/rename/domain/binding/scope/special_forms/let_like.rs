use crate::domain::common_lisp::CommonLispVariableSpecForm;
use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionView, SymbolName};

use crate::domain::rename::selection;

use super::super::super::common_lisp;
use super::super::super::forms::{binding_binds, generic_binding_groups, parameter_form_binds};
use super::super::collect_symbol_atom_spans_unshadowed;

pub(super) fn collect_parallel_let_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };
    if binding_form.delimiter == Some(Delimiter::Bracket) {
        collect_sequential_let_references(view, symbol, output, shadowed_scope_count, input);
        return;
    }
    let Ok(bindings) = generic_binding_groups(binding_form, input) else {
        return;
    };

    for binding in &bindings {
        if let Some(value) = &binding.value {
            collect_symbol_atom_spans_unshadowed(
                value,
                symbol,
                output,
                shadowed_scope_count,
                input,
            );
        }
    }

    if bindings
        .iter()
        .any(|binding| binding_binds(binding, symbol))
    {
        *shadowed_scope_count += 1;
        return;
    }

    for body in &view.children[2..] {
        collect_symbol_atom_spans_unshadowed(body, symbol, output, shadowed_scope_count, input);
    }
}

pub(super) fn collect_value_binding_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    if let Some(value_form) = view.children.get(2) {
        collect_symbol_atom_spans_unshadowed(
            value_form,
            symbol,
            output,
            shadowed_scope_count,
            input,
        );
    }

    if parameter_form_binds(&view.children[1], symbol, input) {
        *shadowed_scope_count += 1;
        return;
    }

    for body in &view.children[3..] {
        collect_symbol_atom_spans_unshadowed(body, symbol, output, shadowed_scope_count, input);
    }
}

pub(super) fn collect_sequential_let_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };
    let Ok(bindings) = generic_binding_groups(binding_form, input) else {
        return;
    };

    for binding in &bindings {
        if let Some(value) = &binding.value {
            collect_symbol_atom_spans_unshadowed(
                value,
                symbol,
                output,
                shadowed_scope_count,
                input,
            );
        }
        if binding_binds(binding, symbol) {
            *shadowed_scope_count += 1;
            return;
        }
    }

    for body in &view.children[2..] {
        collect_symbol_atom_spans_unshadowed(body, symbol, output, shadowed_scope_count, input);
    }
}

pub(super) fn collect_iteration_binding_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };

    if let Some(source_form) = binding_form.children.get(1) {
        collect_symbol_atom_spans_unshadowed(
            source_form,
            symbol,
            output,
            shadowed_scope_count,
            input,
        );
    }

    if iteration_binding_form_binds(binding_form, symbol) {
        *shadowed_scope_count += 1;
        return;
    }

    if let Some(result_form) = binding_form.children.get(2) {
        collect_symbol_atom_spans_unshadowed(
            result_form,
            symbol,
            output,
            shadowed_scope_count,
            input,
        );
    }

    for body in &view.children[2..] {
        collect_symbol_atom_spans_unshadowed(body, symbol, output, shadowed_scope_count, input);
    }
}

pub(super) fn collect_variable_spec_binding_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
    spec_form: CommonLispVariableSpecForm,
    sequential_scope: bool,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };

    if sequential_scope {
        for spec in &binding_form.children {
            if let Some(init_form) = common_lisp::variable_spec_init_form(spec) {
                collect_symbol_atom_spans_unshadowed(
                    init_form,
                    symbol,
                    output,
                    shadowed_scope_count,
                    input,
                );
            }
            if variable_spec_binds(spec, symbol) {
                *shadowed_scope_count += 1;
                return;
            }
        }
    } else {
        for spec in &binding_form.children {
            if let Some(init_form) = common_lisp::variable_spec_init_form(spec) {
                collect_symbol_atom_spans_unshadowed(
                    init_form,
                    symbol,
                    output,
                    shadowed_scope_count,
                    input,
                );
            }
        }
        if binding_form
            .children
            .iter()
            .any(|spec| variable_spec_binds(spec, symbol))
        {
            *shadowed_scope_count += 1;
            return;
        }
    }

    if spec_form.has_step_forms() {
        for spec in &binding_form.children {
            if let Some(step_form) = common_lisp::do_variable_spec_step_form(spec) {
                collect_symbol_atom_spans_unshadowed(
                    step_form,
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

fn iteration_binding_form_binds(binding_form: &ExpressionView, symbol: &SymbolName) -> bool {
    binding_form
        .children
        .first()
        .and_then(selection::atom_text)
        .is_some_and(|name| common_lisp_symbol_reference_eq(name, symbol.as_str()))
}

fn variable_spec_binds(spec: &ExpressionView, symbol: &SymbolName) -> bool {
    selection::atom_text(spec)
        .or_else(|| spec.children.first().and_then(selection::atom_text))
        .is_some_and(|name| common_lisp_symbol_reference_eq(name, symbol.as_str()))
}
