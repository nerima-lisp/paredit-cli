use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, Path};

use super::super::reader::{atom_symbol_span, atom_symbol_text};
use super::super::selection::atom_text;

pub struct CallableNameTarget<'a> {
    pub path: Path,
    pub span: ByteSpan,
    pub text: &'a str,
}

pub fn callable_name_target<'a>(
    view: &'a ExpressionView,
    path: &Path,
) -> Option<CallableNameTarget<'a>> {
    if let Some(text) = atom_symbol_text(view) {
        return Some(CallableNameTarget {
            path: path.clone(),
            span: atom_symbol_span(view).unwrap_or(view.span),
            text,
        });
    }

    let name = setf_callable_name_view(view)?;
    Some(CallableNameTarget {
        path: path.child(1),
        span: atom_symbol_span(name).unwrap_or(name.span),
        text: atom_symbol_text(name)?,
    })
}

fn setf_callable_name_view(view: &ExpressionView) -> Option<&ExpressionView> {
    (view.kind == ExpressionKind::List).then_some(())?;
    let head = view.children.first().and_then(atom_text)?;
    common_lisp_operator_head_eq(head, "setf").then_some(())?;
    view.children
        .get(1)
        .filter(|name| name.kind == ExpressionKind::Atom)
}
