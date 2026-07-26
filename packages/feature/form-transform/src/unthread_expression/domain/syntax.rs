use crate::domain::sexpr::{ExpressionKind, ExpressionView};

pub(super) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

pub(super) fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

pub(super) fn expression_source(input: &str, view: &ExpressionView) -> String {
    view.span.slice(input).to_owned()
}
