use crate::domain::sexpr::ExpressionView;

pub(super) fn list_head(view: &ExpressionView) -> Option<&str> {
    view.children.first().and_then(atom_text)
}

pub(super) fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    view.text.as_deref()
}
