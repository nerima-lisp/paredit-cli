use crate::error::{CallArgumentError, FunctionParameterResult};

use crate::function_parameter::domain::MissingArgumentPolicy;
use crate::function_parameter::domain::list_edit::{
    SpanEdit, atom_text, removal_edit_for_list_item,
};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::validation::resolve_function_call_view;

pub type RemoveArgumentEdit = (ByteSpan, Option<String>, Option<SpanEdit>);

pub fn remove_function_parameter_call_edit(
    input: &str,
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    parameter_index: usize,
    missing_argument_policy: MissingArgumentPolicy,
) -> FunctionParameterResult<RemoveArgumentEdit> {
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
        return Err(CallArgumentError::MissingArgumentAtIndex {
            command: "remove-function-parameter",
            function: function_name.to_string(),
            start: call.view.span.start().get(),
            end: call.view.span.end().get(),
            index: parameter_index,
        }
        .into());
    };
    let removed_argument = argument.span.slice(input).to_owned();
    let edit = removal_edit_for_list_item(input, call.view, argument_item_index)?;
    Ok((call.view.span, Some(removed_argument), Some(edit)))
}

pub fn remove_keyword_function_parameter_call_edit(
    input: &str,
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    keyword: &str,
    positional_prefix_count: usize,
    missing_argument_policy: MissingArgumentPolicy,
) -> FunctionParameterResult<RemoveArgumentEdit> {
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
        return Err(CallArgumentError::KeywordMissing {
            command: "remove-function-parameter",
            function: function_name.to_string(),
            start: call.view.span.start().get(),
            end: call.view.span.end().get(),
            keyword: keyword.to_string(),
        }
        .into());
    }

    let mut found_keyword_item_index = None;
    let mut item_index = first_keyword_item_index;
    while item_index < call.view.children.len() {
        if atom_text(&call.view.children[item_index]).is_some_and(|text| text == keyword)
            && found_keyword_item_index.replace(item_index).is_some()
        {
            return Err(CallArgumentError::DuplicateKeyword {
                command: "remove-function-parameter",
                function: function_name.to_string(),
                start: call.view.span.start().get(),
                end: call.view.span.end().get(),
                keyword: keyword.to_string(),
            }
            .into());
        }
        item_index += 2;
    }

    let Some(keyword_item_index) = found_keyword_item_index else {
        if missing_argument_policy.allows_missing_argument() {
            return Ok((call.view.span, None, None));
        }
        return Err(CallArgumentError::KeywordMissing {
            command: "remove-function-parameter",
            function: function_name.to_string(),
            start: call.view.span.start().get(),
            end: call.view.span.end().get(),
            keyword: keyword.to_string(),
        }
        .into());
    };
    let value_item_index = keyword_item_index + 1;
    let Some(value) = call.view.children.get(value_item_index) else {
        return Err(CallArgumentError::NamedKeywordWithoutValue {
            command: "remove-function-parameter",
            function: function_name.to_string(),
            start: call.view.span.start().get(),
            end: call.view.span.end().get(),
            keyword: keyword.to_string(),
        }
        .into());
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
