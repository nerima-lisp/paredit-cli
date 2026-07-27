use anyhow::{Context, Result};

use crate::domain::function_parameter::calls::resolve_function_call_view;
use crate::domain::sexpr::{ExpressionView, SymbolName};

use super::{ParameterGroup, ReorderableParameter};

pub(in crate::domain::function_parameter) fn ensure_positional_arguments_available(
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    parameters: &[ReorderableParameter],
    required_indices: &[usize],
    command: &str,
) -> Result<()> {
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
            .with_context(|| {
                format!(
                    "{command} parameter '{}' is not aligned with reorderable positional arguments",
                    parameters[*index].name
                )
            })?;
        if argument_count <= positional_index {
            anyhow::bail!(
                "{command} call to '{}' at {}..{} has {} arguments but needs at least {} positional arguments",
                function_name,
                call.view.span.start().get(),
                call.view.span.end().get(),
                argument_count,
                positional_index + 1
            );
        }
    }

    Ok(())
}

pub(in crate::domain::function_parameter) fn argument_for_parameter(
    input: &str,
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    parameter: &ReorderableParameter,
    command: &str,
) -> Result<String> {
    let call = resolve_function_call_view(view, function_name, call_argument_offset, command)?;

    if let Some(call_index) = parameter.call_index {
        let argument = call
            .view
            .children
            .get(call.argument_offset + call_index + 1)
            .with_context(|| {
                format!(
                    "{command} call at {}..{} does not have argument at parameter index {}",
                    call.view.span.start().get(),
                    call.view.span.end().get(),
                    call_index
                )
            })?;
        return Ok(argument.span.slice(input).to_owned());
    }

    let keyword = parameter
        .keyword
        .as_deref()
        .with_context(|| format!("{command} keyword parameter must have keyword metadata"))?;
    let prefix = parameter.positional_prefix_count.with_context(|| {
        format!("{command} keyword parameter must have positional prefix metadata")
    })?;
    let keyword_items = &call.view.children[call.argument_offset + prefix + 1..];
    if keyword_items.len() % 2 != 0 {
        anyhow::bail!(
            "{command} call at {}..{} has an incomplete keyword argument list",
            call.view.span.start().get(),
            call.view.span.end().get()
        );
    }
    for pair in keyword_items.chunks(2) {
        if pair[0].span.slice(input) == keyword {
            return Ok(pair[1].span.slice(input).to_owned());
        }
    }

    anyhow::bail!(
        "{command} call at {}..{} does not contain keyword argument {}",
        call.view.span.start().get(),
        call.view.span.end().get(),
        keyword
    )
}
