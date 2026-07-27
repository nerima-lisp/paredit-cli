use crate::domain::sexpr::{ExpressionKind, ExpressionView};

pub(super) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

pub(super) fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List {
        return None;
    }
    view.children.first().and_then(atom_text)
}

pub(super) fn view_at_span(
    view: &ExpressionView,
    span: crate::domain::sexpr::ByteSpan,
) -> Option<&ExpressionView> {
    if view.span == span {
        return Some(view);
    }
    view.children
        .iter()
        .find_map(|child| view_at_span(child, span))
}
