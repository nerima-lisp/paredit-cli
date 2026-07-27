use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};

pub(super) fn dependency_designator_text(view: &ExpressionView) -> Option<String> {
    atom_symbol_text(view).map(ToOwned::to_owned)
}

pub(super) fn package_qualified_dependency_target(atom: &str) -> Option<String> {
    if atom.starts_with(':')
        || atom.starts_with("#:")
        || atom.starts_with('"')
        || atom.starts_with('\'')
        || atom.contains('/')
        || atom.contains('\\')
    {
        return None;
    }

    let separator_index = atom.find("::").or_else(|| atom.find(':'))?;
    if separator_index == 0 {
        return None;
    }

    let target = &atom[..separator_index];
    (!target.is_empty()).then(|| target.to_owned())
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
