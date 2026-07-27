use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::super::super::common_lisp;
use super::super::super::destructure::binding_pattern_name_spans;
use super::super::collect_symbol_atom_spans_unshadowed;

pub(super) fn collect_loop_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    let mut index = 1usize;

    while index < view.children.len() {
        let child = &view.children[index];

        if common_lisp::loop_keyword_is(child, "for") || common_lisp::loop_keyword_is(child, "as") {
            let binding_index = index + 1;
            let source_start = index + 2;
            let reference_start =
                common_lisp::loop_for_reference_start_index(&view.children, source_start);
            collect_loop_outer_references(
                &view.children[source_start..reference_start],
                symbol,
                output,
                shadowed_scope_count,
                input,
            );

            if view
                .children
                .get(binding_index)
                .is_some_and(|binding| binding_pattern_binds(binding, symbol, input))
            {
                *shadowed_scope_count += 1;
                return;
            }

            index = reference_start;
            continue;
        }

        if common_lisp::loop_keyword_is(child, "with") {
            let binding_index = index + 1;
            let init_start = index + 2;
            let reference_start =
                common_lisp::loop_with_reference_start_index(&view.children, init_start);
            collect_loop_outer_references(
                &view.children[init_start..reference_start],
                symbol,
                output,
                shadowed_scope_count,
                input,
            );

            if view
                .children
                .get(binding_index)
                .is_some_and(|binding| binding_pattern_binds(binding, symbol, input))
            {
                *shadowed_scope_count += 1;
                return;
            }

            index = reference_start;
            continue;
        }

        collect_symbol_atom_spans_unshadowed(child, symbol, output, shadowed_scope_count, input);
        index += 1;
    }
}

fn binding_pattern_binds(pattern: &ExpressionView, symbol: &SymbolName, input: &str) -> bool {
    binding_pattern_name_spans(pattern, input)
        .iter()
        .any(|name| common_lisp_symbol_reference_eq(&name.name, symbol.as_str()))
}

fn collect_loop_outer_references(
    forms: &[ExpressionView],
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    for form in forms {
        if common_lisp::loop_syntax_atom(form) {
            continue;
        }
        collect_symbol_atom_spans_unshadowed(form, symbol, output, shadowed_scope_count, input);
    }
}
