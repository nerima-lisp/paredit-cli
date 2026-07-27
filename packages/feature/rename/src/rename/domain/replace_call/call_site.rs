use crate::domain::definition::definition_shape;
use crate::domain::dialect::Dialect;
use crate::domain::rename::call_identity::call_reference_eq;
use crate::domain::rename::selection::list_head;
use crate::domain::sexpr::{ExpressionView, SymbolName};

use super::ReplaceFunctionCallSite;

pub(super) fn replace_call_site_from_view(
    view: &ExpressionView,
    dialect: Dialect,
    input: &str,
    path: String,
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
        path,
        head_span,
        span: view.span,
        replacement: to.as_str().to_owned(),
        text: view.span.slice(input).to_owned(),
    })
}
