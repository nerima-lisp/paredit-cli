mod reader;
mod special_forms;

use crate::domain::common_lisp::{common_lisp_operator_head_eq, common_lisp_symbol_reference_eq};
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix, SymbolName};

use reader::{
    apply_reader_prefix_context, atom_symbol_span, atom_symbol_text, collect_explicit_reader_form,
};

pub(in crate::domain::rename) fn collect_symbol_atom_spans_unshadowed(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    collect_symbol_atom_spans_unshadowed_in_context(
        view,
        symbol,
        output,
        shadowed_scope_count,
        input,
        0,
        true,
    );
}

/// Symbol-macro renaming has no notion of a `(declare (special ...))` form,
/// so declared-special specifiers must not be treated as references.
pub(in crate::domain::rename) fn collect_symbol_atom_spans_unshadowed_ignoring_declared_specials(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    collect_symbol_atom_spans_unshadowed_in_context(
        view,
        symbol,
        output,
        shadowed_scope_count,
        input,
        0,
        false,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_symbol_atom_spans_unshadowed_in_context(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
    quasiquote_depth: usize,
    collect_declared_specials: bool,
) {
    let Some(quasiquote_depth) = apply_reader_prefix_context(view, quasiquote_depth) else {
        return;
    };

    if view.kind == ExpressionKind::Atom {
        if view.reader_prefixes.contains(&ReaderPrefix::Function) {
            return;
        }

        if quasiquote_depth == 0
            && atom_symbol_text(view)
                .is_some_and(|text| common_lisp_symbol_reference_eq(text, symbol.as_str()))
        {
            if let Some(span) = atom_symbol_span(view) {
                output.push(span);
            }
        }
        return;
    }

    if collect_declared_specials {
        if collect_common_lisp_declaration_form_references(view, symbol, output) {
            return;
        }
    } else if view
        .children
        .first()
        .and_then(super::super::selection::atom_text)
        .is_some_and(crate::domain::common_lisp::is_common_lisp_declaration_form)
    {
        return;
    }

    if collect_explicit_reader_form(
        view,
        symbol,
        output,
        shadowed_scope_count,
        input,
        quasiquote_depth,
        collect_declared_specials,
    ) {
        return;
    }

    if quasiquote_depth > 0 {
        for child in &view.children {
            collect_symbol_atom_spans_unshadowed_in_context(
                child,
                symbol,
                output,
                shadowed_scope_count,
                input,
                quasiquote_depth,
                collect_declared_specials,
            );
        }
        return;
    }

    if collect_shadow_aware_special_form(view, symbol, output, shadowed_scope_count, input) {
        return;
    }

    for child in &view.children {
        collect_symbol_atom_spans_unshadowed_in_context(
            child,
            symbol,
            output,
            shadowed_scope_count,
            input,
            0,
            collect_declared_specials,
        );
    }
}

pub(super) fn collect_shadow_aware_special_form(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) -> bool {
    special_forms::collect_shadow_aware_special_form(
        view,
        symbol,
        output,
        shadowed_scope_count,
        input,
    )
}

fn collect_common_lisp_declaration_form_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
) -> bool {
    if view.kind != ExpressionKind::List {
        return false;
    }

    let Some(head) = view
        .children
        .first()
        .and_then(super::super::selection::atom_text)
    else {
        return false;
    };

    if common_lisp_operator_head_eq(head, "declare") {
        for declaration in &view.children[1..] {
            collect_common_lisp_special_specifier_references(declaration, symbol, output);
        }
        return true;
    }

    if common_lisp_operator_head_eq(head, "declaim") {
        for declaration in &view.children[1..] {
            collect_common_lisp_special_specifier_references(declaration, symbol, output);
        }
        return true;
    }

    if common_lisp_operator_head_eq(head, "proclaim") {
        for declaration in &view.children[1..] {
            if declaration.reader_prefixes.contains(&ReaderPrefix::Quote) {
                collect_common_lisp_special_specifier_references(declaration, symbol, output);
            }
        }
        return true;
    }

    false
}

fn collect_common_lisp_special_specifier_references(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
) {
    if view.kind != ExpressionKind::List {
        return;
    }

    let Some(head) = view
        .children
        .first()
        .and_then(super::super::selection::atom_text)
    else {
        return;
    };

    if !common_lisp_operator_head_eq(head, "special") {
        return;
    }

    for declared in &view.children[1..] {
        if declared.kind == ExpressionKind::Atom
            && atom_symbol_text(declared)
                .is_some_and(|text| common_lisp_symbol_reference_eq(text, symbol.as_str()))
        {
            if let Some(span) = atom_symbol_span(declared) {
                output.push(span);
            }
        }
    }
}
