use crate::domain::common_lisp::{
    common_lisp_operator_head_eq, common_lisp_symbol_reference_eq,
    normalize_common_lisp_operator_head,
};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::reader::apply_reader_prefix_context;
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix, SymbolName};

mod binding_forms;
mod body;
mod lambda_lists;

use binding_forms::collect_shadow_aware_special_form;

pub fn collect_unshadowed_symbol_references(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    collect_unshadowed_symbol_references_in_context(dialect, view, symbol, input, output, 0);
}

pub(super) fn collect_unshadowed_symbol_references_in_context(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    quasiquote_depth: usize,
) {
    let Some(quasiquote_depth) = apply_reader_prefix_context(view, quasiquote_depth) else {
        return;
    };

    if view.kind == ExpressionKind::Atom {
        collect_atom_reference(dialect, view, symbol, output, quasiquote_depth);
        return;
    }

    if collect_explicit_reader_form(dialect, view, symbol, input, output, quasiquote_depth) {
        return;
    }

    if quasiquote_depth > 0 {
        collect_quasiquoted_children(dialect, view, symbol, input, output, quasiquote_depth);
        return;
    }

    if collect_shadow_aware_special_form(dialect, view, symbol, input, output) {
        return;
    }

    for child in &view.children {
        collect_unshadowed_symbol_references_in_context(dialect, child, symbol, input, output, 0);
    }
}

/// Common Lisp is the only case-insensitive dialect per the CLHS reader, and it
/// is the only one where a `cl:`-style package prefix may be stripped. Every
/// other dialect (Clojure, Scheme, Fennel, Janet, Emacs Lisp) is case-sensitive,
/// so matching must be exact to avoid rewriting an unrelated `myFn` when editing
/// `myfn`, or matching a package-qualified `cl:foo` against a local `foo`.
pub(super) fn symbol_name_matches(dialect: Dialect, candidate: &str, symbol: &str) -> bool {
    match dialect {
        Dialect::CommonLisp => common_lisp_symbol_reference_eq(candidate, symbol),
        _ => candidate == symbol,
    }
}

fn collect_atom_reference(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    quasiquote_depth: usize,
) {
    if view.reader_prefixes.contains(&ReaderPrefix::Function) {
        return;
    }

    if quasiquote_depth == 0
        && super::syntax::atom_symbol_text(view)
            .is_some_and(|text| symbol_name_matches(dialect, text, symbol.as_str()))
    {
        if let Some(span) = super::syntax::atom_symbol_span(view) {
            output.push(span);
        }
    }
}

fn collect_quasiquoted_children(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    quasiquote_depth: usize,
) {
    for child in &view.children {
        collect_unshadowed_symbol_references_in_context(
            dialect,
            child,
            symbol,
            input,
            output,
            quasiquote_depth,
        );
    }
}

fn collect_explicit_reader_form(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
    quasiquote_depth: usize,
) -> bool {
    if view.kind != ExpressionKind::List || view.children.len() < 2 {
        return false;
    }

    let Some(head) = super::syntax::atom_text(&view.children[0]) else {
        return false;
    };

    let normalized_head = normalize_common_lisp_operator_head(head);

    if common_lisp_operator_head_eq(normalized_head, "quote") {
        return true;
    }

    // `(function name)` is the explicit spelling of `#'name`: a
    // function-namespace designator naming exactly one symbol, opaque to
    // this value-namespace scan. `(function (arg-types...) return-type)` is
    // the unrelated `FUNCTION` *type specifier* used in `declaim`/`the`/
    // `check-type` position (e.g. `(declaim (ftype (function (my-word)
    // my-word) f)))`) — it has the same head but a list, not a symbol, as
    // its second element, and its contents are ordinary type-position atoms
    // that must still be scanned normally.
    if common_lisp_operator_head_eq(normalized_head, "function")
        && view.children.len() == 2
        && view.children[1].kind == ExpressionKind::Atom
    {
        return true;
    }

    match normalized_head.to_ascii_lowercase().as_str() {
        "quasiquote" => {
            for child in &view.children[1..] {
                collect_unshadowed_symbol_references_in_context(
                    dialect,
                    child,
                    symbol,
                    input,
                    output,
                    quasiquote_depth + 1,
                );
            }
            true
        }
        "unquote" | "unquote-splicing" if quasiquote_depth > 0 => {
            for child in &view.children[1..] {
                collect_unshadowed_symbol_references_in_context(
                    dialect,
                    child,
                    symbol,
                    input,
                    output,
                    quasiquote_depth - 1,
                );
            }
            true
        }
        _ => false,
    }
}
