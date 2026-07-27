use crate::domain::common_lisp::{
    common_lisp_macro_expander_path, common_lisp_operator_head_eq,
    normalize_common_lisp_operator_head,
};
use crate::domain::dialect::Dialect;
use crate::domain::rename::selection::list_head;
use crate::domain::sexpr::reader::atom_text;
pub(crate) use crate::domain::sexpr::reader::{
    apply_reader_prefix_context, atom_symbol_span, atom_symbol_text,
};
use crate::domain::sexpr::{ExpressionKind, ExpressionView, Path, SyntaxTree};

pub(crate) fn executable_reader_context_at_path(
    tree: &SyntaxTree,
    dialect: Dialect,
    path: &Path,
) -> anyhow::Result<bool> {
    let mut quasiquote_depth = 0;
    let indexes = path.to_raw_indexes();

    for end in 1..=indexes.len() {
        let ancestor = Path::from_indexes(indexes[..end].to_vec());
        let view = tree.select_path(&ancestor)?.view();
        let Some(depth) = apply_reader_prefix_context(&view, quasiquote_depth) else {
            return Ok(false);
        };
        quasiquote_depth = depth;
    }

    Ok(quasiquote_depth == 0 || common_lisp_macro_expander_path(tree, dialect, path)?)
}

pub(crate) fn explicit_reader_form_kind(view: &ExpressionView) -> Option<String> {
    if view.kind != ExpressionKind::List || view.children.len() < 2 {
        return None;
    }

    let head = atom_text(&view.children[0])?;
    Some(normalize_common_lisp_operator_head(head).to_ascii_lowercase())
}

pub(crate) fn explicit_reader_function_lambda_view(
    view: &ExpressionView,
) -> Option<&ExpressionView> {
    if explicit_reader_form_kind(view)?.as_str() != "function" {
        return None;
    }

    let lambda_view = view.children.get(1)?;
    list_head(lambda_view)
        .is_some_and(|head| common_lisp_operator_head_eq(head, "lambda"))
        .then_some(lambda_view)
}

pub(crate) fn explicit_reader_function_lambda_body_children(
    view: &ExpressionView,
) -> Option<impl Iterator<Item = (usize, &ExpressionView)>> {
    explicit_reader_function_lambda_view(view)
        .map(|lambda_view| lambda_view.children.iter().enumerate().skip(2))
}

/// Body children of a bare `(lambda (params...) body...)` form, skipping the
/// lambda-list at index 1 so it is never visited as if it were call/reference
/// position. `lambda` is a standard macro that expands to `(function (lambda
/// ...))` (CLHS 3.1.2.1.2.4), so a bare lambda introduces the same
/// callable-namespace scope as its reader-quoted spelling and must skip its
/// parameter list the same way `explicit_reader_function_lambda_body_children`
/// does for the `#'(lambda ...)` case.
pub(crate) fn bare_lambda_body_children(
    view: &ExpressionView,
) -> Option<impl Iterator<Item = (usize, &ExpressionView)>> {
    let head = list_head(view)?;
    if !common_lisp_operator_head_eq(head, "lambda") {
        return None;
    }
    Some(view.children.iter().enumerate().skip(2))
}
