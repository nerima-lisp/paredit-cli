use paredit_core_syntax::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView};

pub const fn spans_overlap(left: ByteSpan, right: ByteSpan) -> bool {
    left.start().get() < right.end().get() && right.start().get() < left.end().get()
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
