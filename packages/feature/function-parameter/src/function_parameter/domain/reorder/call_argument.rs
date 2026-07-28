use crate::error::{CallArgumentError, FunctionParameterResult, ParameterSelectionError};

use crate::function_parameter::domain::calls::resolve_function_call_view;
use paredit_core_syntax::sexpr::{ExpressionView, SymbolName};

use super::{ParameterGroup, ReorderableParameter};

pub fn ensure_positional_arguments_available(
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    parameters: &[ReorderableParameter],
    required_indices: &[usize],
    command: &'static str,
) -> FunctionParameterResult<()> {
    let call = resolve_function_call_view(view, function_name, call_argument_offset, command)?;
    let positional_parameters = parameters
        .iter()
        .filter(|parameter| parameter.group != ParameterGroup::Keyword)
        .collect::<Vec<_>>();
    let argument_count = call
        .view
        .children
        .len()
        .saturating_sub(call.argument_offset + 1);

    for index in required_indices {
        if parameters[*index].group == ParameterGroup::Keyword {
            continue;
        }
        let positional_index = positional_parameters
            .iter()
            .position(|candidate| candidate.item_index == parameters[*index].item_index)
            .ok_or_else(|| ParameterSelectionError::NotAlignedWithPositional {
                command,
                name: parameters[*index].name.to_string(),
            })?;
        if argument_count <= positional_index {
            return Err(CallArgumentError::TooFewArguments {
                command,
                function: function_name.to_string(),
                start: call.view.span.start().get(),
                end: call.view.span.end().get(),
                actual: argument_count,
                needed: positional_index + 1,
            }
            .into());
        }
    }

    Ok(())
}

pub fn argument_for_parameter(
    input: &str,
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    parameter: &ReorderableParameter,
    command: &'static str,
) -> FunctionParameterResult<String> {
    let call = resolve_function_call_view(view, function_name, call_argument_offset, command)?;

    if let Some(call_index) = parameter.call_index {
        let argument = call
            .view
            .children
            .get(call.argument_offset + call_index + 1)
            .ok_or_else(|| CallArgumentError::UnnamedMissingArgumentAtIndex {
                command,
                start: call.view.span.start().get(),
                end: call.view.span.end().get(),
                index: call_index,
            })?;
        return Ok(argument.span.slice(input).to_owned());
    }

    let keyword = parameter
        .keyword
        .as_deref()
        .ok_or(CallArgumentError::KeywordMetadataMissing { command })?;
    let prefix = parameter
        .positional_prefix_count
        .ok_or(CallArgumentError::PositionalPrefixMetadataMissing { command })?;
    let keyword_items = &call.view.children[call.argument_offset + prefix + 1..];
    if keyword_items.len() % 2 != 0 {
        return Err(CallArgumentError::UnnamedIncompleteKeywordList {
            command,
            start: call.view.span.start().get(),
            end: call.view.span.end().get(),
        }
        .into());
    }
    for pair in keyword_items.chunks(2) {
        if pair[0].span.slice(input) == keyword {
            return Ok(pair[1].span.slice(input).to_owned());
        }
    }

    Err(CallArgumentError::UnnamedKeywordMissing {
        command,
        start: call.view.span.start().get(),
        end: call.view.span.end().get(),
        keyword: keyword.to_string(),
    }
    .into())
}
