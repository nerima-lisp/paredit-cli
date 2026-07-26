use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView};

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

pub fn expression_source(input: &str, view: &ExpressionView) -> String {
    view.span.slice(input).to_owned()
}

pub fn is_threadable_call(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && view.delimiter == Some(Delimiter::Paren)
        && view.children.len() >= 2
        && list_head(view).is_some()
}
