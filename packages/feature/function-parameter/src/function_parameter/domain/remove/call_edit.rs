use anyhow::Result;

use crate::domain::function_parameter::MissingArgumentPolicy;
use crate::domain::function_parameter::calls::{
    remove_function_parameter_call_edit, remove_keyword_function_parameter_call_edit,
};
use crate::domain::function_parameter::list_edit::SpanEdit;
use crate::domain::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::metadata::RemoveParameterMetadata;

pub(super) struct RemoveCallEdit {
    pub(super) span: ByteSpan,
    pub(super) removed_argument: Option<String>,
    pub(super) edit: Option<SpanEdit>,
}

pub(super) fn remove_call_argument_edit(
    input: &str,
    call_view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    parameter: &RemoveParameterMetadata,
    missing_argument_policy: MissingArgumentPolicy,
) -> Result<RemoveCallEdit> {
    let (span, removed_argument, edit) =
        if let Some(keyword) = parameter.parameter_keyword.as_deref() {
            remove_keyword_function_parameter_call_edit(
                input,
                call_view,
                function_name,
                call_argument_offset,
                keyword,
                parameter.parameter_index,
                missing_argument_policy,
            )?
        } else if parameter.dotted_tail {
            (call_view.span, None, None)
        } else {
            remove_function_parameter_call_edit(
                input,
                call_view,
                function_name,
                call_argument_offset,
                parameter.parameter_index,
                missing_argument_policy,
            )?
        };

    Ok(RemoveCallEdit {
        span,
        removed_argument,
        edit,
    })
}
