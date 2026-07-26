use crate::domain::common_lisp::common_lisp_operator_head_eq;
use crate::domain::sexpr::{ExpressionKind, ExpressionView, Path};

use super::DefinitionNameTarget;

pub(super) fn definition_name_text(view: &ExpressionView) -> Option<&str> {
    atom_text(view)
        .or_else(|| setf_callable_name_text(view))
        .or_else(|| list_head_name_text(view))
}

pub(super) fn definition_name_target<'a>(
    view: &'a ExpressionView,
    path: &Path,
) -> Option<DefinitionNameTarget<'a>> {
    if let Some(text) = atom_text(view) {
        return Some(DefinitionNameTarget {
            path: path.clone(),
            span: view.span,
            text,
        });
    }

    if let Some(name) = setf_callable_name_view(view) {
        return Some(DefinitionNameTarget {
            path: path.child(1),
            span: name.span,
            text: atom_text(name)?,
        });
    }

    let name = list_head_name_view(view)?;
    Some(DefinitionNameTarget {
        path: path.child(0),
        span: name.span,
        text: atom_text(name)?,
    })
}

fn setf_callable_name_text(view: &ExpressionView) -> Option<&str> {
    setf_callable_name_view(view).and_then(atom_text)
}

fn setf_callable_name_view(view: &ExpressionView) -> Option<&ExpressionView> {
    (view.kind == ExpressionKind::List).then_some(())?;
    let head = view.children.first().and_then(atom_text)?;
    common_lisp_operator_head_eq(head, "setf").then_some(())?;
    view.children
        .get(1)
        .filter(|name| name.kind == ExpressionKind::Atom)
}

/// Handles the `defstruct`-style `(name option*)` name slot (CLHS 3.4.13):
/// the struct name is the list's first element, followed by struct options
/// such as `(:constructor make-line)`. Excludes a `(setf name)` list, which
/// `setf_callable_name_view` already handles and means something different.
fn list_head_name_text(view: &ExpressionView) -> Option<&str> {
    list_head_name_view(view).and_then(atom_text)
}

fn list_head_name_view(view: &ExpressionView) -> Option<&ExpressionView> {
    (view.kind == ExpressionKind::List).then_some(())?;
    let head = view.children.first()?;
    let head_text = atom_text(head)?;
    (!common_lisp_operator_head_eq(head_text, "setf")).then_some(head)
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}
