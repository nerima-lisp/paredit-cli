use crate::domain::common_lisp::CommonLispOperator;
use crate::domain::common_lisp::common_lisp_operator_head_eq;

use super::{ByteOffset, ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix};

pub(crate) fn apply_reader_prefix_context(
    view: &ExpressionView,
    mut quasiquote_depth: usize,
) -> Option<usize> {
    if view
        .reader_prefixes
        .iter()
        .any(|prefix| prefix.is_opaque_reader_form())
    {
        return None;
    }
    let has_function_prefix = view.reader_prefixes.contains(&ReaderPrefix::Function);

    for prefix in &view.reader_prefixes {
        match prefix {
            // A top-level quote (`quasiquote_depth == 0`) is genuinely
            // inert data: `,`/`,@` only have meaning inside an active
            // quasiquote, so a bare `'x` can never contain a live
            // reference and stays fully opaque. But `',x` — a quote
            // wrapping an unquote, the standard idiom for splicing a
            // computed value as a literal into a macro's generated code,
            // e.g. `` `(setf (get ',name 'prop) ',computed-value) `` — is
            // only reachable while already inside a quasiquote template
            // (`quasiquote_depth > 0`). There, the quote itself does not
            // block traversal: it must keep descending so the nested
            // unquote is still found as a live reference.
            ReaderPrefix::Quote => {
                if quasiquote_depth == 0 {
                    return None;
                }
            }
            ReaderPrefix::Function => {}
            ReaderPrefix::Quasiquote => quasiquote_depth += 1,
            ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing => {
                quasiquote_depth = quasiquote_depth.saturating_sub(1);
            }
            ReaderPrefix::ReadEval => return None,
            // `#(...)`/`#{...}`, `^...`, and `#?(...)`/`#?@(...)` carry live
            // code or references in at least one supported dialect (Clojure
            // anonymous functions and metadata targets), so treat them like
            // `Function` rather than opaque data: keep traversing normally
            // instead of hiding the contents from rename/reference tracking.
            ReaderPrefix::HashLiteral
            | ReaderPrefix::Metadata
            | ReaderPrefix::ReaderConditional
            | ReaderPrefix::ReaderConditionalSplicing => {}
        }
    }

    if has_function_prefix
        && view.kind == ExpressionKind::List
        && !is_lambda_like_function_list(view)
    {
        return None;
    }

    Some(quasiquote_depth)
}

pub(crate) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

pub(crate) fn atom_symbol_text(view: &ExpressionView) -> Option<&str> {
    atom_text(view).and_then(|text| text.get(view.symbol_offset..))
}

pub(crate) fn atom_symbol_span(view: &ExpressionView) -> Option<ByteSpan> {
    (view.kind == ExpressionKind::Atom).then(|| {
        let start = view.span.start().get() + view.symbol_offset;
        ByteSpan::new(ByteOffset::new(start), view.span.end())
    })
}

fn is_lambda_like_function_list(view: &ExpressionView) -> bool {
    let Some(head) = view.children.first().and_then(atom_symbol_text) else {
        return false;
    };

    CommonLispOperator::from_head(head).is_some_and(|operator| operator.is_lambda_like())
        || common_lisp_operator_head_eq(head, "setf")
}
