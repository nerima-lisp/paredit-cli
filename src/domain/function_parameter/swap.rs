use anyhow::{Context, Result};

use crate::domain::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::sexpr::SyntaxTree;

use super::calls::{
    FunctionCallPathRequest, reorder_function_parameter_call_edit, resolve_function_call_paths,
};
use super::definition::{
    find_unique_parameter_location, parse_swap_function_parameters_definition,
};
use super::list_edit::{
    SpanEdit, apply_byte_span_edits, ensure_non_overlapping_spans, spans_overlap,
};
use super::reorder::call_argument::{
    argument_for_parameter, ensure_positional_arguments_available,
};
use super::reorder::{
    build_new_relative_order, ensure_parameter_is_reorderable,
    ensure_reorder_stays_within_parameter_groups, reorder_function_definition_edit,
    reorderable_parameters,
};
use super::types::{SwapFunctionParametersPlan, SwapFunctionParametersRequest};

pub fn plan_swap_function_parameters(
    request: SwapFunctionParametersRequest<'_>,
) -> Result<SwapFunctionParametersPlan> {
    if request.left_name == request.right_name {
        anyhow::bail!("swap-function-parameters requires two distinct parameter names");
    }

    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let target = parse_swap_function_parameters_definition(
        request.dialect,
        &tree,
        &request.definition_path,
    )?;
    let left =
        find_unique_parameter_location(&target, &request.left_name, "swap-function-parameters")?;
    let right =
        find_unique_parameter_location(&target, &request.right_name, "swap-function-parameters")?;
    let reorderable_parameters =
        reorderable_parameters(&target.parameters, "swap-function-parameters")?;
    let old_parameter_order = reorderable_parameters
        .iter()
        .map(|parameter| parameter.name.clone())
        .collect::<Vec<_>>();
    let left_index = ensure_parameter_is_reorderable(
        &reorderable_parameters,
        left.item_index,
        &request.left_name,
        "swap-function-parameters",
    )?;
    let right_index = ensure_parameter_is_reorderable(
        &reorderable_parameters,
        right.item_index,
        &request.right_name,
        "swap-function-parameters",
    )?;
    let mut new_parameter_order = old_parameter_order
        .iter()
        .map(Clone::clone)
        .collect::<Vec<_>>();
    new_parameter_order.swap(left_index, right_index);
    let new_relative_order = build_new_relative_order(&old_parameter_order, &new_parameter_order)?;
    ensure_reorder_stays_within_parameter_groups(
        &reorderable_parameters,
        &new_relative_order,
        "swap-function-parameters",
    )?;
    let call_paths = resolve_function_call_paths(FunctionCallPathRequest {
        tree: &tree,
        dialect: request.dialect,
        explicit_call_paths: request.call_paths,
        all_calls: request.all_calls,
        definition_span: target.definition_span,
        definition_scope: target.definition_scope,
        function_name: &target.function_name,
        command: "swap-function-parameters",
    })?;

    let mut edits = Vec::<SpanEdit>::with_capacity(call_paths.len() + 1);
    edits.push(reorder_function_definition_edit(
        request.input,
        &tree,
        &target.parameter_container,
        &reorderable_parameters,
        &new_relative_order,
        "swap-function-parameters",
    )?);

    let mut call_spans = Vec::with_capacity(call_paths.len());
    let mut swapped_arguments = Vec::with_capacity(call_paths.len());
    for call_path in &call_paths {
        let call_selection = tree.select_path(call_path)?;
        let call_view = call_selection.view();
        if spans_overlap(target.definition_span, call_selection.span()) {
            anyhow::bail!(
                "swap-function-parameters call path {call_path} overlaps the selected definition"
            );
        }
        ensure_positional_arguments_available(
            &call_view,
            &target.function_name,
            target.call_argument_offset,
            &reorderable_parameters,
            &[left_index, right_index],
            "swap-function-parameters",
        )?;
        let original_left_argument = argument_for_parameter(
            request.input,
            &call_view,
            &target.function_name,
            target.call_argument_offset,
            &reorderable_parameters[left_index],
            "swap-function-parameters",
        )?;
        let original_right_argument = argument_for_parameter(
            request.input,
            &call_view,
            &target.function_name,
            target.call_argument_offset,
            &reorderable_parameters[right_index],
            "swap-function-parameters",
        )?;
        let (call_span, _reordered_arguments, edit) = reorder_function_parameter_call_edit(
            request.input,
            &call_view,
            &target.function_name,
            target.call_argument_offset,
            &reorderable_parameters,
            &new_relative_order,
            "swap-function-parameters",
        )?;
        call_spans.push(call_span);
        swapped_arguments.push((original_left_argument, original_right_argument));
        if let Some(edit) = edit {
            edits.push(edit);
        }
    }

    let mut sorted_call_spans = call_spans.clone();
    sorted_call_spans.sort_by_key(|span| span.start());
    ensure_non_overlapping_spans(sorted_call_spans)?;
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("swap-function-parameters output is not a valid S-expression document")?;

    let changed = rewritten != request.input;
    Ok(SwapFunctionParametersPlan {
        dialect: request.dialect,
        definition_path: request.definition_path,
        call_paths,
        all_calls: request.all_calls,
        definition_span: target.definition_span,
        parameter_list_span: target.parameter_container.span,
        call_spans,
        function_name: target.function_name,
        left_name: request.left_name,
        right_name: request.right_name,
        left_index,
        right_index,
        swapped_arguments,
        rewritten,
        changed,
    })
}
