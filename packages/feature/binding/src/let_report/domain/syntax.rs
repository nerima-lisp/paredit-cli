use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView};

pub(super) fn view_at_span(view: &ExpressionView, span: ByteSpan) -> Option<&ExpressionView> {
    if view.span == span {
        return Some(view);
    }
    view.children
        .iter()
        .find_map(|child| view_at_span(child, span))
}

pub(super) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

pub(super) fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

pub(super) fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return None;
    }

    atom_child(view, 0)
}
