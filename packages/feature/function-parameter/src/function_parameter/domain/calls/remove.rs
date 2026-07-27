use anyhow::Result;

use crate::domain::function_parameter::MissingArgumentPolicy;
use crate::domain::function_parameter::list_edit::{
    SpanEdit, atom_text, removal_edit_for_list_item,
};
use crate::domain::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::validation::resolve_function_call_view;

pub(in crate::domain::function_parameter) type RemoveArgumentEdit =
    (ByteSpan, Option<String>, Option<SpanEdit>);

pub(in crate::domain::function_parameter) fn remove_function_parameter_call_edit(
    input: &str,
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    parameter_index: usize,
    missing_argument_policy: MissingArgumentPolicy,
) -> Result<RemoveArgumentEdit> {
    let call = resolve_function_call_view(
        view,
        function_name,
        call_argument_offset,
        "remove-function-parameter",
    )?;

    let argument_item_index = call.argument_offset + parameter_index + 1;
    let Some(argument) = call.view.children.get(argument_item_index) else {
        if missing_argument_policy.allows_missing_argument() {
            return Ok((call.view.span, None, None));
        }
        anyhow::bail!(
            "remove-function-parameter call to '{}' at {}..{} does not have argument at parameter index {}",
            function_name,
            call.view.span.start().get(),
            call.view.span.end().get(),
            parameter_index
        );
    };
    let removed_argument = argument.span.slice(input).to_owned();
    let edit = removal_edit_for_list_item(input, call.view, argument_item_index)?;
    Ok((call.view.span, Some(removed_argument), Some(edit)))
}

pub(in crate::domain::function_parameter) fn remove_keyword_function_parameter_call_edit(
    input: &str,
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    keyword: &str,
    positional_prefix_count: usize,
    missing_argument_policy: MissingArgumentPolicy,
) -> Result<RemoveArgumentEdit> {
    let call = resolve_function_call_view(
        view,
        function_name,
        call_argument_offset,
        "remove-function-parameter",
    )?;

    let first_keyword_item_index = call.argument_offset + positional_prefix_count + 1;
    if first_keyword_item_index >= call.view.children.len() {
        if missing_argument_policy.allows_missing_argument() {
            return Ok((call.view.span, None, None));
        }
        anyhow::bail!(
            "remove-function-parameter call to '{}' at {}..{} does not have keyword argument {}",
            function_name,
            call.view.span.start().get(),
            call.view.span.end().get(),
            keyword
        );
    }

    let mut found_keyword_item_index = None;
    let mut item_index = first_keyword_item_index;
    while item_index < call.view.children.len() {
        if atom_text(&call.view.children[item_index]).is_some_and(|text| text == keyword)
            && found_keyword_item_index.replace(item_index).is_some()
        {
            anyhow::bail!(
                "remove-function-parameter call to '{}' at {}..{} contains duplicate keyword argument {}",
                function_name,
                call.view.span.start().get(),
                call.view.span.end().get(),
                keyword
            );
        }
        item_index += 2;
    }

    let Some(keyword_item_index) = found_keyword_item_index else {
        if missing_argument_policy.allows_missing_argument() {
            return Ok((call.view.span, None, None));
        }
        anyhow::bail!(
            "remove-function-parameter call to '{}' at {}..{} does not have keyword argument {}",
            function_name,
            call.view.span.start().get(),
            call.view.span.end().get(),
            keyword
        );
    };
    let value_item_index = keyword_item_index + 1;
    let Some(value) = call.view.children.get(value_item_index) else {
        anyhow::bail!(
            "remove-function-parameter call to '{}' at {}..{} has keyword {} without a value",
            function_name,
            call.view.span.start().get(),
            call.view.span.end().get(),
            keyword
        );
    };

    let keyword_item = &call.view.children[keyword_item_index];
    let previous = &call.view.children[keyword_item_index - 1];
    let removed_argument = format!(
        "{} {}",
        keyword_item.span.slice(input),
        value.span.slice(input)
    );
    let edit = (
        ByteSpan::new(previous.span.end(), value.span.end()),
        String::new(),
    );
    Ok((call.view.span, Some(removed_argument), Some(edit)))
}
