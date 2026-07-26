use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};

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

pub(super) fn expression_source(input: &str, view: &ExpressionView) -> String {
    view.span.slice(input).to_owned()
}

pub(super) fn is_threadable_call(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && view.delimiter == Some(Delimiter::Paren)
        && view.children.len() >= 2
        && list_head(view).is_some()
}
