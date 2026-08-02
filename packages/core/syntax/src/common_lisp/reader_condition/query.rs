#[cfg(test)]
use crate::sexpr::ExpressionPath;
use crate::sexpr::{ByteOffset, ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};

#[cfg(test)]
use super::CommonLispReaderConditionalDispatch;
use super::{CommonLispReaderConditionalForm, CommonLispReaderConditionalKind};

/// Returns the two-byte `#+` or `#-` dispatch of every Common Lisp reader
/// conditional, in source order.
///
/// There is no dispatch *node* to return: every reader consumes the dispatch,
/// the feature expression, and the guarded datum as one opaque atom, so this
/// reports the leading two bytes of that atom's span. Callers that want the
/// conditional's whole extent want [`common_lisp_reader_conditional_forms`].
///
/// Rejecting incomplete input is no longer this function's job. It used to be
/// — the permissive reader accepted a bare `#+` and this reported it so a
/// caller could refuse before refactoring — but the parser now refuses the
/// document itself, in every dialect, which no caller can forget to check.
#[cfg(test)]
#[must_use]
pub fn common_lisp_reader_conditional_dispatches(
    tree: &SyntaxTree,
) -> Vec<CommonLispReaderConditionalDispatch> {
    let mut dispatches = Vec::new();
    collect_dispatches(
        &tree.root_view(),
        &ExpressionPath::from_indexes(Vec::new()),
        &mut dispatches,
    );
    dispatches
}

/// Returns the complete source region consumed by every reader conditional.
///
/// That region is exactly the opaque atom's own span: every reader, the
/// permissive `Dialect::Unknown` one included, consumes the dispatch, the
/// feature expression, and the guarded datum as a single node, so no sibling
/// scan is needed to find where the conditional ends.
///
/// `paredit_core_edit::mutation_safety::reader_condition` refuses any Common
/// Lisp edit that *partially* overlaps a returned span, so a span narrower
/// than the real conditional would look like a clean containment and
/// authorise an edit that cuts the conditional in half.
#[must_use]
pub fn common_lisp_reader_conditional_forms(
    tree: &SyntaxTree,
) -> Vec<CommonLispReaderConditionalForm> {
    let mut forms = Vec::new();
    collect_forms(&tree.root_view(), &mut forms);
    forms
}

#[cfg(test)]
fn collect_dispatches(
    view: &ExpressionView,
    path: &ExpressionPath,
    dispatches: &mut Vec<CommonLispReaderConditionalDispatch>,
) {
    if let Some((kind, span)) = reader_conditional(view) {
        dispatches.push(CommonLispReaderConditionalDispatch {
            kind,
            path: path.clone(),
            span,
        });
    }

    for (index, child) in view.children.iter().enumerate() {
        collect_dispatches(child, &path.child(index), dispatches);
    }
}

fn collect_forms(view: &ExpressionView, forms: &mut Vec<CommonLispReaderConditionalForm>) {
    // A `(node, next child)` stack rather than recursion: document nesting is
    // attacker-controlled, and popping the child before the parent's next
    // index keeps the results in source order.
    let mut stack = vec![(view, 0)];
    while let Some((view, index)) = stack.pop() {
        let Some(child) = view.children.get(index) else {
            continue;
        };
        if let Some((kind, dispatch_span)) = reader_conditional(child) {
            forms.push(CommonLispReaderConditionalForm {
                kind,
                dispatch_span,
                // The conditional *is* the node. This used to scan forward
                // over the next two siblings to find the end, because the
                // permissive reader split a conditional into three of them.
                span: child.span,
            });
        }

        stack.push((view, index + 1));
        stack.push((child, 0));
    }
}

#[must_use]
pub fn reader_conditional_kind(view: &ExpressionView) -> Option<CommonLispReaderConditionalKind> {
    reader_conditional(view).map(|(kind, _)| kind)
}

/// The polarity and dispatch span of `view`, if it is a reader conditional.
///
/// There was a third component here, a `ReaderConditionalShape`, telling
/// [`collect_forms`] whether the conditional was one node or three siblings.
/// The `LegacyDispatch` half of it fired only on an atom whose text is exactly
/// `#+` or `#-`, and no reader produces one any more. `DialectReaderPolicy`
/// classifies `#+` as a `MultiDatum` dispatch in Common Lisp, LFE, Hy, Carp
/// and the permissive reader; as an `UnsupportedDispatch` in Emacs Lisp,
/// Scheme and Racket; as a tagged literal in Clojure; as a line comment in
/// Janet; and as a one-byte function prefix in Fennel, where `symbol_offset`
/// then steps past the `#`. Every one of those either consumes a payload, or
/// fails, or leaves text that does not begin `#+`.
///
/// Checked as well as reasoned about: brute-forcing every string of length 5
/// or less over `# + - ( ) space a ; ' | \ "` across all eleven dialects
/// produced 1,165,924 successful parses and not one bare dispatch atom.
fn reader_conditional(
    view: &ExpressionView,
) -> Option<(CommonLispReaderConditionalKind, ByteSpan)> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }

    let text = view.text.as_deref()?.get(view.symbol_offset..)?;
    let kind = match text {
        text if text.starts_with("#+") => CommonLispReaderConditionalKind::Include,
        text if text.starts_with("#-") => CommonLispReaderConditionalKind::Exclude,
        _ => return None,
    };
    let dispatch_start = view.content_span.start();
    let dispatch_end = ByteOffset::new(dispatch_start.get() + 2);

    Some((kind, ByteSpan::new(dispatch_start, dispatch_end)))
}
