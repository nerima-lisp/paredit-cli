use crate::rename::domain::call_identity::call_reference_eq;
use crate::rename::domain::selection::list_head;
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionView, SymbolName};

use super::ReplaceFunctionCallSite;

pub fn replace_call_site_from_view(
    view: &ExpressionView,
    dialect: Dialect,
    input: &str,
    path: impl FnOnce() -> String,
    from: &SymbolName,
    to: &SymbolName,
) -> Option<ReplaceFunctionCallSite> {
    let head = list_head(view)?;
    if !call_reference_eq(dialect, head, from.as_str())
        || definition_shape(dialect, view, head).is_some()
    {
        return None;
    }
    let head_span = view.children.first().map(|child| child.span)?;

    Some(ReplaceFunctionCallSite {
        path: path(),
        head_span,
        span: view.span,
        replacement: to.as_str().to_owned(),
        text: view.span.slice(input).to_owned(),
    })
}
