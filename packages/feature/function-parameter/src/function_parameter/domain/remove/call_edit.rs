use anyhow::Result;

use crate::function_parameter::domain::MissingArgumentPolicy;
use crate::function_parameter::domain::calls::{
    remove_function_parameter_call_edit, remove_keyword_function_parameter_call_edit,
};
use crate::function_parameter::domain::list_edit::SpanEdit;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::metadata::RemoveParameterMetadata;

pub struct RemoveCallEdit {
    pub span: ByteSpan,
    pub removed_argument: Option<String>,
    pub edit: Option<SpanEdit>,
}

pub fn remove_call_argument_edit(
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
