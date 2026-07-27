use paredit_core_syntax::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView};

pub fn view_at_span(view: &ExpressionView, span: ByteSpan) -> Option<&ExpressionView> {
    if view.span == span {
        return Some(view);
    }
    view.children
        .iter()
        .find_map(|child| view_at_span(child, span))
}

pub fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

pub fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

pub fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return None;
    }

    atom_child(view, 0)
}
