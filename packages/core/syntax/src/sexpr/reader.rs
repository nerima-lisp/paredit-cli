use crate::common_lisp::CommonLispOperator;
use crate::common_lisp::common_lisp_operator_head_eq;

use super::{ByteOffset, ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix};

#[must_use]
pub fn apply_reader_prefix_context(
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
            // LFE's `#B(…)`, `#M(…)` and `#S(…)` sit here for the same reason:
            // their elements are ordinary expressions, not opaque data. A
            // binary segment is `(value (size N) (unit 8))` and a map literal's
            // values are arbitrary forms, so rename and reference tracking must
            // keep descending into them.
            //
            // Carp's `&x`/`@x`/`~x`/`$[...]` join them for the same reason and
            // more directly: each wraps an ordinary subexpression -- `(ref x)`,
            // `(copy x)`, a deref, a static array literal -- so `x` is a live
            // reference that rename and reference tracking must still see.
            // Treating them as opaque would hide roughly a fifth of every Carp
            // file from those commands.
            ReaderPrefix::HashLiteral
            | ReaderPrefix::LfeBinary
            | ReaderPrefix::LfeMap
            | ReaderPrefix::LfeStruct
            | ReaderPrefix::Metadata
            | ReaderPrefix::ReaderConditional
            | ReaderPrefix::ReaderConditionalSplicing
            | ReaderPrefix::Ref
            | ReaderPrefix::Copy
            | ReaderPrefix::Deref
            | ReaderPrefix::StaticArray => {}
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

#[must_use]
pub fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

#[must_use]
pub fn atom_symbol_text(view: &ExpressionView) -> Option<&str> {
    atom_text(view).and_then(|text| text.get(view.symbol_offset..))
}

#[must_use]
pub fn atom_symbol_span(view: &ExpressionView) -> Option<ByteSpan> {
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
