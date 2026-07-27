use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView};

pub fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

pub fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List {
        return None;
    }
    view.children.first().and_then(atom_text)
}

pub fn view_at_span(
    view: &ExpressionView,
    span: paredit_core_syntax::sexpr::ByteSpan,
) -> Option<&ExpressionView> {
    if view.span == span {
        return Some(view);
    }
    view.children
        .iter()
        .find_map(|child| view_at_span(child, span))
}
