#[cfg(test)]
use crate::domain::sexpr::ExpressionPath;
use crate::domain::sexpr::{ByteOffset, ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};

#[cfg(test)]
use super::CommonLispReaderConditionalDispatch;
use super::{CommonLispReaderConditionalForm, CommonLispReaderConditionalKind};

/// Returns every Common Lisp `#+` or `#-` dispatch atom in source order.
///
/// Legacy trees keep the dispatch, feature expression, and guarded datum as
/// siblings. Dialect-aware Common Lisp trees keep the complete conditional as
/// one opaque atom. A bare legacy dispatch is still reported so callers can
/// reject incomplete input safely before attempting a structural refactor.
#[cfg(test)]
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
/// This supports both legacy trees, where the dispatch, feature expression,
/// and guarded datum are siblings, and dialect-aware Common Lisp trees, where
/// the complete conditional is one opaque atom.
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
    if let Some((kind, span, _)) = reader_conditional(view) {
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
    let mut stack = vec![(view, 0)];
    while let Some((view, index)) = stack.pop() {
        let Some(child) = view.children.get(index) else {
            continue;
        };
        if let Some((kind, dispatch_span, shape)) = reader_conditional(child) {
            let span = match shape {
                ReaderConditionalShape::OpaqueForm => child.span,
                ReaderConditionalShape::LegacyDispatch => {
                    let end = view
                        .children
                        .get(index + 2)
                        .or_else(|| view.children.get(index + 1))
                        .map_or(child.span.end(), |component| component.span.end());
                    ByteSpan::new(child.span.start(), end)
                }
            };
            forms.push(CommonLispReaderConditionalForm {
                kind,
                dispatch_span,
                span,
            });
        }

        stack.push((view, index + 1));
        stack.push((child, 0));
    }
}

pub(crate) fn reader_conditional_kind(
    view: &ExpressionView,
) -> Option<CommonLispReaderConditionalKind> {
    reader_conditional(view).map(|(kind, _, _)| kind)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReaderConditionalShape {
    LegacyDispatch,
    OpaqueForm,
}

fn reader_conditional(
    view: &ExpressionView,
) -> Option<(
    CommonLispReaderConditionalKind,
    ByteSpan,
    ReaderConditionalShape,
)> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }

    let text = view.text.as_deref()?.get(view.symbol_offset..)?;
    let (kind, shape) = match text {
        "#+" => (
            CommonLispReaderConditionalKind::Include,
            ReaderConditionalShape::LegacyDispatch,
        ),
        "#-" => (
            CommonLispReaderConditionalKind::Exclude,
            ReaderConditionalShape::LegacyDispatch,
        ),
        text if text.starts_with("#+") => (
            CommonLispReaderConditionalKind::Include,
            ReaderConditionalShape::OpaqueForm,
        ),
        text if text.starts_with("#-") => (
            CommonLispReaderConditionalKind::Exclude,
            ReaderConditionalShape::OpaqueForm,
        ),
        _ => return None,
    };
    let dispatch_start = view.content_span.start();
    let dispatch_end = ByteOffset::new(dispatch_start.get() + 2);

    Some((kind, ByteSpan::new(dispatch_start, dispatch_end), shape))
}
