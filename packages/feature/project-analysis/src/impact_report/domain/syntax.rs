use paredit_core_syntax::sexpr::ExpressionView;

pub fn list_head(view: &ExpressionView) -> Option<&str> {
    view.children.first().and_then(atom_text)
}

pub fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    view.text.as_deref()
}
