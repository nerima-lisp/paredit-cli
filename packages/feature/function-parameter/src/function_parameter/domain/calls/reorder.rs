use anyhow::{Context, Result};

use crate::domain::function_parameter::list_edit::SpanEdit;
use crate::domain::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::super::reorder::{ParameterGroup, ReorderableParameter};
use super::validation::resolve_function_call_view;

pub(in crate::domain::function_parameter) type ReorderArgumentEdit =
    (ByteSpan, Vec<String>, Option<SpanEdit>);

pub(in crate::domain::function_parameter) fn reorder_function_parameter_call_edit(
    input: &str,
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    parameters: &[ReorderableParameter],
    new_relative_order: &[usize],
    command: &str,
) -> Result<ReorderArgumentEdit> {
    let call = resolve_function_call_view(view, function_name, call_argument_offset, command)?;

    let mut reordered_arguments = call.view.children[call.argument_offset + 1..]
        .iter()
        .map(|child| child.span.slice(input).to_owned())
        .collect::<Vec<_>>();

    reorder_positional_arguments(
        call.view,
        function_name,
        parameters,
        new_relative_order,
        &mut reordered_arguments,
        command,
    )?;
    reorder_keyword_arguments(
        call.view,
        function_name,
        parameters,
        new_relative_order,
        &mut reordered_arguments,
        command,
    )?;

    let old_arguments = call.view.children[call.argument_offset + 1..]
        .iter()
        .map(|child| child.span.slice(input).to_owned())
        .collect::<Vec<_>>();
    if reordered_arguments == old_arguments {
        return Ok((call.view.span, reordered_arguments, None));
    }

    let start = call.view.span.start().get();
    let end = call.view.span.end().get();
    let open = &input[start..start + 1];
    let close = &input[end - 1..end];
    let mut items = vec![call.view.children[0].span.slice(input).to_owned()];
    if call.argument_offset > 0 {
        items.extend(
            call.view.children[1..call.argument_offset + 1]
                .iter()
                .map(|child| child.span.slice(input).to_owned()),
        );
    }
    items.extend(reordered_arguments.iter().cloned());
    let edit = (
        call.view.span,
        if items.is_empty() {
            format!("{open}{close}")
        } else {
            format!("{open}{}{close}", items.join(" "))
        },
    );
    Ok((call.view.span, reordered_arguments, Some(edit)))
}

fn reorder_positional_arguments(
    view: &ExpressionView,
    function_name: &SymbolName,
    parameters: &[ReorderableParameter],
    new_relative_order: &[usize],
    reordered_arguments: &mut [String],
    command: &str,
) -> Result<()> {
    let positional_parameters = parameters
        .iter()
        .filter(|parameter| parameter.group != ParameterGroup::Keyword)
        .collect::<Vec<_>>();
    if positional_parameters.is_empty() {
        return Ok(());
    }

    let positional_argument_count = positional_parameters.len();
    let argument_count = reordered_arguments.len();
    if argument_count < positional_argument_count {
        anyhow::bail!(
            "{command} call to '{}' at {}..{} has {} arguments but needs at least {} positional arguments",
            function_name,
            view.span.start().get(),
            view.span.end().get(),
            argument_count,
            positional_argument_count
        );
    }

    let original_positional_arguments = reordered_arguments[..positional_argument_count].to_vec();
    let positional_relative_order = new_relative_order
        .iter()
        .copied()
        .filter(|&index| parameters[index].group != ParameterGroup::Keyword)
        .collect::<Vec<_>>();
    for (new_index, old_index) in positional_relative_order.into_iter().enumerate() {
        let old_call_index = parameters[old_index]
            .call_index
            .with_context(|| {
                format!(
                    "{command} metadata for '{}' at {}..{} is missing call_index for positional parameter '{}'",
                    function_name,
                    view.span.start().get(),
                    view.span.end().get(),
                    parameters[old_index].name
                )
            })?;
        reordered_arguments[new_index] = original_positional_arguments[old_call_index].clone();
    }

    Ok(())
}

fn reorder_keyword_arguments(
    view: &ExpressionView,
    function_name: &SymbolName,
    parameters: &[ReorderableParameter],
    new_relative_order: &[usize],
    reordered_arguments: &mut Vec<String>,
    command: &str,
) -> Result<()> {
    let keyword_parameters = parameters
        .iter()
        .filter(|parameter| parameter.group == ParameterGroup::Keyword)
        .collect::<Vec<_>>();
    if keyword_parameters.is_empty() {
        return Ok(());
    }

    let positional_prefix_count = keyword_parameters[0]
        .positional_prefix_count
        .with_context(|| {
            format!(
                "{command} metadata for '{}' at {}..{} is missing positional_prefix_count for keyword parameter '{}'",
                function_name,
                view.span.start().get(),
                view.span.end().get(),
                keyword_parameters[0].name
            )
        })?;
    if reordered_arguments.len() < positional_prefix_count {
        anyhow::bail!(
            "{command} call to '{}' at {}..{} has {} arguments but needs at least {} positional arguments before keyword arguments",
            function_name,
            view.span.start().get(),
            view.span.end().get(),
            reordered_arguments.len(),
            positional_prefix_count
        );
    }

    let keyword_items = reordered_arguments.split_off(positional_prefix_count);
    if keyword_items.len() % 2 != 0 {
        anyhow::bail!(
            "{command} call to '{}' at {}..{} has an incomplete keyword argument list",
            function_name,
            view.span.start().get(),
            view.span.end().get()
        );
    }

    let mut known_pairs = std::collections::BTreeMap::new();
    let mut unknown_pairs = Vec::new();
    for pair in keyword_items.chunks(2) {
        let keyword = &pair[0];
        let value = &pair[1];
        let is_known_keyword = keyword_parameters
            .iter()
            .any(|parameter| parameter.keyword.as_deref() == Some(keyword.as_str()));
        if is_known_keyword {
            if known_pairs
                .insert(keyword.clone(), vec![keyword.clone(), value.clone()])
                .is_some()
            {
                anyhow::bail!(
                    "{command} call to '{}' at {}..{} contains duplicate keyword argument {}",
                    function_name,
                    view.span.start().get(),
                    view.span.end().get(),
                    keyword
                );
            }
        } else {
            unknown_pairs.push(keyword.clone());
            unknown_pairs.push(value.clone());
        }
    }

    let keyword_relative_order = new_relative_order
        .iter()
        .copied()
        .filter(|&index| parameters[index].group == ParameterGroup::Keyword)
        .collect::<Vec<_>>();
    for old_index in keyword_relative_order {
        let keyword = parameters[old_index].keyword.as_deref().with_context(|| {
            format!(
                "{command} metadata for '{}' at {}..{} is missing keyword name for parameter '{}'",
                function_name,
                view.span.start().get(),
                view.span.end().get(),
                parameters[old_index].name
            )
        })?;
        if let Some(pair) = known_pairs.remove(keyword) {
            reordered_arguments.extend(pair);
        }
    }
    reordered_arguments.extend(unknown_pairs);

    Ok(())
}
