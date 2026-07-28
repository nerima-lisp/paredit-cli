#[cfg(test)]
use crate::sexpr::ExpressionPath;
use crate::sexpr::{ByteOffset, ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};

#[cfg(test)]
use super::CommonLispReaderLabelDispatch;
use super::{CommonLispReaderLabelForm, CommonLispReaderLabelKind};

/// Returns every Common Lisp `#n=` or `#n#` dispatch atom in source order.
#[cfg(test)]
#[must_use]
pub fn common_lisp_reader_label_dispatches(
    tree: &SyntaxTree,
) -> Vec<CommonLispReaderLabelDispatch> {
    let mut dispatches = Vec::new();
    collect_dispatches(
        &tree.root_view(),
        &ExpressionPath::from_indexes(Vec::new()),
        &mut dispatches,
    );
    dispatches
}

/// Returns the complete source region consumed by every reader-label form.
///
/// Both tree shapes are supported, the same way
/// [`common_lisp_reader_conditional_forms`](crate::common_lisp::common_lisp_reader_conditional_forms)
/// supports both. A legacy tree keeps a `#n=` dispatch and the datum it labels
/// as siblings; a dialect-aware Common Lisp tree keeps the whole `#n=(…)` as
/// one opaque atom, because the reader consumed the datum and there is nothing
/// left for it to be a sibling of.
#[must_use]
pub fn common_lisp_reader_label_forms(tree: &SyntaxTree) -> Vec<CommonLispReaderLabelForm> {
    let mut forms = Vec::new();
    collect_forms(&tree.root_view(), &mut forms);
    forms
}

#[cfg(test)]
fn collect_dispatches(
    view: &ExpressionView,
    path: &ExpressionPath,
    dispatches: &mut Vec<CommonLispReaderLabelDispatch>,
) {
    if let Some((kind, dispatch_span, _)) = reader_label(view) {
        dispatches.push(CommonLispReaderLabelDispatch {
            kind,
            path: path.clone(),
            span: dispatch_span,
        });
    }

    for (index, child) in view.children.iter().enumerate() {
        collect_dispatches(child, &path.child(index), dispatches);
    }
}

fn collect_forms(view: &ExpressionView, forms: &mut Vec<CommonLispReaderLabelForm>) {
    let mut stack = vec![(view, 0)];
    while let Some((view, index)) = stack.pop() {
        let Some(child) = view.children.get(index) else {
            continue;
        };
        if let Some((kind, dispatch_span, shape)) = reader_label(child) {
            let span = match (kind, shape) {
                // Only a legacy definition needs its datum stitched on. An
                // opaque form already spans its own datum, and a reference is
                // complete on its own.
                (CommonLispReaderLabelKind::Definition, ReaderLabelShape::LegacyDispatch) => {
                    view.children.get(index + 1).map_or(child.span, |datum| {
                        ByteSpan::new(child.span.start(), datum.span.end())
                    })
                }
                _ => child.span,
            };
            forms.push(CommonLispReaderLabelForm {
                kind,
                dispatch_span,
                span,
            });
        }

        stack.push((view, index + 1));
        stack.push((child, 0));
    }
}

#[must_use]
pub fn reader_label_kind(view: &ExpressionView) -> Option<CommonLispReaderLabelKind> {
    reader_label(view).map(|(kind, _, _)| kind)
}

/// How the tree spells one reader label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReaderLabelShape {
    /// `#n=` alone, with the datum it labels as the next sibling.
    LegacyDispatch,
    /// `#n=(…)` as one atom.
    OpaqueForm,
}

/// Reads an atom as a reader label: its kind, the span of the `#n=`/`#n#`
/// dispatch itself, and which shape the tree used.
fn reader_label(
    view: &ExpressionView,
) -> Option<(CommonLispReaderLabelKind, ByteSpan, ReaderLabelShape)> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }

    let text = view
        .text
        .as_deref()
        .and_then(|text| text.get(view.symbol_offset..))?;

    // A label consumed together with a quote family prefix — `'#1=(a b)` —
    // scans as one atom whose `symbol_offset` is zero, because the reader took
    // the whole thing in one go and there was no separate prefix node to
    // record. Stepping over the prefix here is what lets that spelling be
    // recognised; `#'` is deliberately not in the set, since `#'#1=` is not
    // syntax the reader accepts.
    let quoted = text.len() - text.trim_start_matches(['\'', '`', ',', '@']).len();
    let suffix = text.get(quoted..)?.strip_prefix('#')?;

    let digits = suffix
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(suffix.len());
    if digits == 0 {
        return None;
    }

    let (kind, shape) = match suffix.as_bytes().get(digits)? {
        b'=' => (
            CommonLispReaderLabelKind::Definition,
            if suffix.len() == digits + 1 {
                ReaderLabelShape::LegacyDispatch
            } else {
                ReaderLabelShape::OpaqueForm
            },
        ),
        // A reference consumes nothing after itself, so trailing text means
        // this is some other `#` syntax that merely starts the same way.
        b'#' if suffix.len() == digits + 1 => (
            CommonLispReaderLabelKind::Reference,
            ReaderLabelShape::LegacyDispatch,
        ),
        _ => return None,
    };

    // `#12=` is a four-byte dispatch, not a three-byte one, so the end is
    // computed from the digit count rather than assumed.
    let start = ByteOffset::new(view.content_span.start().get() + quoted);
    let end = ByteOffset::new(start.get() + 1 + digits + 1);
    Some((kind, ByteSpan::new(start, end), shape))
}
