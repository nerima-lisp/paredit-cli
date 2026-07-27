use crate::domain::dialect::Dialect;
use crate::domain::rename::call_identity::call_reference_eq;
use crate::domain::sexpr::{ExpressionView, SymbolName};

use super::{WrapFunctionCallSite, WrapFunctionCallTemplate};
use crate::domain::rename::selection::list_head;

pub(super) fn wrap_call_site_from_view(
    view: &ExpressionView,
    dialect: Dialect,
    input: &str,
    path: String,
    function: &SymbolName,
    wrapper: &SymbolName,
    template: Option<&WrapFunctionCallTemplate>,
) -> Option<WrapFunctionCallSite> {
    let head = list_head(view)?;
    if !call_reference_eq(dialect, head, function.as_str()) {
        return None;
    }
    let text = view.content_span.slice(input).to_owned();
    let replacement = match template {
        Some(template) => template.apply(&text).ok()?,
        None => format!("({} {})", wrapper.as_str(), text),
    };
    Some(WrapFunctionCallSite {
        path,
        span: view.content_span,
        replacement,
        text,
    })
}
