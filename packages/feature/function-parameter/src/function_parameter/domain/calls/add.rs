use anyhow::Result;

use crate::domain::function_parameter::FunctionParameterInsert;
use crate::domain::function_parameter::list_edit::{atom_text, insertion_edit_for_list_item};
use crate::domain::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::validation::resolve_function_call_view;

pub(in crate::domain::function_parameter) fn add_function_parameter_call_edit(
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    argument: &str,
    insert: FunctionParameterInsert,
) -> Result<(ByteSpan, String)> {
    let call = resolve_function_call_view(
        view,
        function_name,
        call_argument_offset,
        "add-function-parameter",
    )?;

    insertion_edit_for_list_item(call.view, call.argument_offset + 1, argument, insert)
}

pub(in crate::domain::function_parameter) fn add_positional_function_parameter_call_edit(
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    argument: &str,
    argument_index: usize,
) -> Result<(ByteSpan, String)> {
    let call = resolve_function_call_view(
        view,
        function_name,
        call_argument_offset,
        "add-function-parameter",
    )?;

    let insertion_item_index = call.argument_offset + argument_index + 1;
    if insertion_item_index > call.view.children.len() {
        anyhow::bail!(
            "add-function-parameter call to '{}' at {}..{} does not have {} positional argument(s) before rest arguments",
            function_name,
            call.view.span.start().get(),
            call.view.span.end().get(),
            argument_index
        );
    }

    insertion_edit_for_list_item(
        call.view,
        insertion_item_index,
        argument,
        FunctionParameterInsert::Start,
    )
}

pub(in crate::domain::function_parameter) fn add_optional_function_parameter_call_edit(
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    argument: &str,
    positional_prefix_count: usize,
    argument_index: usize,
) -> Result<(ByteSpan, String)> {
    let call = resolve_function_call_view(
        view,
        function_name,
        call_argument_offset,
        "add-function-parameter",
    )?;

    let first_keyword_item_index = call
        .view
        .children
        .iter()
        .enumerate()
        .skip(call.argument_offset + positional_prefix_count + 1)
        .find_map(|(item_index, child)| {
            atom_text(child)
                .is_some_and(|text| text.starts_with(':'))
                .then_some(item_index)
        });
    let positional_argument_count = first_keyword_item_index
        .unwrap_or(call.view.children.len())
        .saturating_sub(call.argument_offset + 1);
    if argument_index > positional_argument_count {
        anyhow::bail!(
            "add-function-parameter call to '{}' at {}..{} does not have {} positional argument(s) before optional argument",
            function_name,
            call.view.span.start().get(),
            call.view.span.end().get(),
            argument_index
        );
    }

    let insertion_item_index = call.argument_offset + argument_index + 1;
    if insertion_item_index > call.view.children.len() {
        anyhow::bail!(
            "add-function-parameter call to '{}' at {}..{} does not have {} positional argument(s) before optional argument",
            function_name,
            call.view.span.start().get(),
            call.view.span.end().get(),
            argument_index
        );
    }

    insertion_edit_for_list_item(
        call.view,
        insertion_item_index,
        argument,
        FunctionParameterInsert::Start,
    )
}

pub(in crate::domain::function_parameter) fn add_keyword_function_parameter_call_edit(
    view: &ExpressionView,
    function_name: &SymbolName,
    call_argument_offset: usize,
    keyword: &str,
    argument: &str,
    positional_prefix_count: usize,
    insert: FunctionParameterInsert,
) -> Result<(ByteSpan, String)> {
    let call = resolve_function_call_view(
        view,
        function_name,
        call_argument_offset,
        "add-function-parameter",
    )?;

    let first_keyword_item_index = call.argument_offset + positional_prefix_count + 1;
    if first_keyword_item_index > call.view.children.len() {
        anyhow::bail!(
            "add-function-parameter call to '{}' at {}..{} does not have {} positional argument(s) before keyword arguments",
            function_name,
            call.view.span.start().get(),
            call.view.span.end().get(),
            positional_prefix_count
        );
    }

    let mut item_index = first_keyword_item_index;
    while item_index < call.view.children.len() {
        if atom_text(&call.view.children[item_index]).is_some_and(|text| text == keyword) {
            anyhow::bail!(
                "add-function-parameter call to '{}' at {}..{} already contains keyword argument {}",
                function_name,
                call.view.span.start().get(),
                call.view.span.end().get(),
                keyword
            );
        }
        if item_index + 1 >= call.view.children.len() {
            anyhow::bail!(
                "add-function-parameter call to '{}' at {}..{} has keyword argument without a value",
                function_name,
                call.view.span.start().get(),
                call.view.span.end().get()
            );
        }
        item_index += 2;
    }

    let argument_pair = format!("{keyword} {argument}");
    let insertion_index = match insert {
        FunctionParameterInsert::Start => first_keyword_item_index,
        FunctionParameterInsert::End => call.view.children.len(),
    };
    insertion_edit_for_list_item(
        call.view,
        insertion_index,
        &argument_pair,
        FunctionParameterInsert::Start,
    )
}
