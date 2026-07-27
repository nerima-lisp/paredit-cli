use anyhow::Result;

use super::super::RenameAtNamespace;
use super::super::selection::{AtomPathIndex, ancestor_views};
use crate::domain::common_lisp::common_lisp_operator_head_eq;
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, Path};

pub(super) fn enclosing_specialized_scope(
    root_view: &ExpressionView,
    path: &Path,
    namespace: RenameAtNamespace,
) -> Result<Option<ByteSpan>> {
    let indexes = path.to_raw_indexes();
    for (end, view) in ancestor_views(root_view, path)?
        .into_iter()
        .enumerate()
        .rev()
    {
        let end = end + 1;
        if view.kind != ExpressionKind::List {
            continue;
        }
        let Some(head) = view
            .children
            .first()
            .and_then(|child| child.text.as_deref())
        else {
            continue;
        };
        let matches = match namespace {
            RenameAtNamespace::LocalFunction => {
                common_lisp_operator_head_eq(head, "flet")
                    || common_lisp_operator_head_eq(head, "labels")
            }
            RenameAtNamespace::Macro => {
                common_lisp_operator_head_eq(head, "macrolet")
                    || common_lisp_operator_head_eq(head, "compiler-macrolet")
            }
            RenameAtNamespace::SymbolMacro => common_lisp_operator_head_eq(head, "symbol-macrolet"),
            _ => false,
        };
        let descendants = &indexes[end..];
        let definition_body_without_self_scope = descendants.len() >= 3
            && descendants[0] == 1
            && descendants[2] >= 2
            && (common_lisp_operator_head_eq(head, "flet")
                || common_lisp_operator_head_eq(head, "macrolet")
                || common_lisp_operator_head_eq(head, "compiler-macrolet"));
        let symbol_macro_expansion = descendants.len() >= 3
            && descendants[0] == 1
            && descendants[2] >= 1
            && common_lisp_operator_head_eq(head, "symbol-macrolet");
        if matches && !definition_body_without_self_scope && !symbol_macro_expansion {
            return Ok(Some(view.span));
        }
    }
    Ok(None)
}

pub(super) fn occurrence_has_scope(
    root_view: &ExpressionView,
    atom_paths: AtomPathIndex<'_>,
    span: ByteSpan,
    namespace: RenameAtNamespace,
    expected: Option<ByteSpan>,
) -> bool {
    let Some(path) = atom_paths.path_for_span(span) else {
        return false;
    };
    enclosing_specialized_scope(root_view, &path, namespace).ok() == Some(expected)
}
