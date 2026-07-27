use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SymbolName};

use super::super::super::selection::atom_text;
use super::collect_symbol_atom_spans_unshadowed_in_context;
use crate::domain::common_lisp::common_lisp_operator_head_eq;
pub(super) use crate::domain::rename::reader::{
    apply_reader_prefix_context, atom_symbol_span, atom_symbol_text,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_explicit_reader_form(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
    quasiquote_depth: usize,
    collect_declared_specials: bool,
) -> bool {
    if view.kind != ExpressionKind::List || view.children.len() < 2 {
        return false;
    }

    let Some(head) = view.children.first().and_then(atom_text) else {
        return false;
    };

    if common_lisp_operator_head_eq(head, "quote") || common_lisp_operator_head_eq(head, "function")
    {
        return true;
    }

    match crate::domain::common_lisp::normalize_common_lisp_operator_head(head)
        .to_ascii_lowercase()
        .as_str()
    {
        "quasiquote" => {
            for child in &view.children[1..] {
                collect_symbol_atom_spans_unshadowed_in_context(
                    child,
                    symbol,
                    output,
                    shadowed_scope_count,
                    input,
                    quasiquote_depth + 1,
                    collect_declared_specials,
                );
            }
            true
        }
        "unquote" | "unquote-splicing" if quasiquote_depth > 0 => {
            for child in &view.children[1..] {
                collect_symbol_atom_spans_unshadowed_in_context(
                    child,
                    symbol,
                    output,
                    shadowed_scope_count,
                    input,
                    quasiquote_depth - 1,
                    collect_declared_specials,
                );
            }
            true
        }
        _ => false,
    }
}
