use anyhow::Result;

use crate::domain::function_parameter::RemoveFunctionParameterRequest;
use crate::domain::function_parameter::definition::{
    FunctionParameterTarget, find_unique_parameter_location,
};
use crate::domain::function_parameter::list_edit::{
    SpanEdit, is_dotted_list_separator, removal_edit_for_list_item,
};

#[derive(Debug, Clone)]
pub(super) struct RemoveParameterMetadata {
    pub(super) definition_edit: SpanEdit,
    pub(super) parameter_index: usize,
    pub(super) parameter_keyword: Option<String>,
    pub(super) dotted_tail: bool,
}

pub(super) fn resolve_remove_parameter_metadata(
    target: &FunctionParameterTarget,
    request: &RemoveFunctionParameterRequest<'_>,
) -> Result<RemoveParameterMetadata> {
    let parameter =
        find_unique_parameter_location(target, &request.name, "remove-function-parameter")?;
    let parameter_item_index = parameter.item_index;
    let parameter_index = parameter
        .call_index
        .or_else(|| {
            parameter
                .keyword_argument
                .as_ref()
                .map(|keyword| keyword.positional_prefix_count)
        })
        .unwrap_or(parameter_item_index);
    let parameter_keyword = parameter
        .keyword_argument
        .as_ref()
        .map(|keyword| keyword.keyword.clone());
    let dotted_tail = parameter_item_index > 0
        && target
            .parameter_container
            .children
            .get(parameter_item_index - 1)
            .is_some_and(is_dotted_list_separator);

    Ok(RemoveParameterMetadata {
        definition_edit: removal_edit_for_list_item(
            request.input,
            &target.parameter_container,
            parameter_item_index,
        )?,
        parameter_index,
        parameter_keyword,
        dotted_tail,
    })
}
