use anyhow::{Context, Result};

use crate::domain::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::sexpr::SyntaxTree;

use super::calls::{
    FunctionCallPathRequest, reorder_function_parameter_call_edit, resolve_function_call_paths,
};
use super::definition::{find_unique_parameter_location, parse_move_function_parameter_definition};
use super::list_edit::{
    SpanEdit, apply_byte_span_edits, ensure_non_overlapping_spans, spans_overlap,
};
use super::reorder::call_argument::argument_for_parameter;
use super::reorder::{
    build_new_relative_order, ensure_parameter_is_reorderable,
    ensure_reorder_stays_within_parameter_groups, reorder_function_definition_edit,
    reorderable_parameters,
};
use super::types::{MoveFunctionParameterPlan, MoveFunctionParameterRequest};

pub fn plan_move_function_parameter(
    request: MoveFunctionParameterRequest<'_>,
) -> Result<MoveFunctionParameterPlan> {
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let target =
        parse_move_function_parameter_definition(request.dialect, &tree, &request.definition_path)?;
    let parameter =
        find_unique_parameter_location(&target, &request.name, "move-function-parameter")?;
    let reorderable_parameters =
        reorderable_parameters(&target.parameters, "move-function-parameter")?;
    let old_parameter_order = reorderable_parameters
        .iter()
        .map(|parameter| parameter.name.clone())
        .collect::<Vec<_>>();
    let from_index = ensure_parameter_is_reorderable(
        &reorderable_parameters,
        parameter.item_index,
        &request.name,
        "move-function-parameter",
    )?;
    let parameter_count = reorderable_parameters.len();
    if request.to_index >= parameter_count {
        anyhow::bail!(
            "move-function-parameter target index {} is out of bounds for {} parameters",
            request.to_index,
            parameter_count
        );
    }
    let mut new_parameter_order = old_parameter_order
        .iter()
        .map(Clone::clone)
        .collect::<Vec<_>>();
    let moved = new_parameter_order.remove(from_index);
    new_parameter_order.insert(request.to_index, moved);
    let new_relative_order = build_new_relative_order(&old_parameter_order, &new_parameter_order)?;
    ensure_reorder_stays_within_parameter_groups(
        &reorderable_parameters,
        &new_relative_order,
        "move-function-parameter",
    )?;
    let call_paths = resolve_function_call_paths(FunctionCallPathRequest {
        tree: &tree,
        dialect: request.dialect,
        explicit_call_paths: request.call_paths,
        all_calls: request.all_calls,
        definition_span: target.definition_span,
        definition_scope: target.definition_scope,
        function_name: &target.function_name,
        command: "move-function-parameter",
    })?;

    let mut edits = Vec::<SpanEdit>::with_capacity(call_paths.len() + 1);
    if from_index != request.to_index {
        edits.push(reorder_function_definition_edit(
            request.input,
            &tree,
            &target.parameter_container,
            &reorderable_parameters,
            &new_relative_order,
            "move-function-parameter",
        )?);
    }

    let mut call_spans = Vec::with_capacity(call_paths.len());
    let mut moved_arguments = Vec::with_capacity(call_paths.len());
    for call_path in &call_paths {
        let call_selection = tree.select_path(call_path)?;
        let call_view = call_selection.view();
        if spans_overlap(target.definition_span, call_selection.span()) {
            anyhow::bail!(
                "move-function-parameter call path {call_path} overlaps the selected definition"
            );
        }
        let moved_argument = argument_for_parameter(
            request.input,
            &call_view,
            &target.function_name,
            target.call_argument_offset,
            &reorderable_parameters[from_index],
            "move-function-parameter",
        )?;
        let (call_span, _reordered_arguments, edit) = reorder_function_parameter_call_edit(
            request.input,
            &call_view,
            &target.function_name,
            target.call_argument_offset,
            &reorderable_parameters,
            &new_relative_order,
            "move-function-parameter",
        )?;
        call_spans.push(call_span);
        moved_arguments.push(moved_argument);
        if let Some(edit) = edit {
            edits.push(edit);
        }
    }

    let mut sorted_call_spans = call_spans.clone();
    sorted_call_spans.sort_by_key(|span| span.start());
    ensure_non_overlapping_spans(sorted_call_spans)?;
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("move-function-parameter output is not a valid S-expression document")?;

    let changed = rewritten != request.input;
    Ok(MoveFunctionParameterPlan {
        dialect: request.dialect,
        definition_path: request.definition_path,
        call_paths,
        all_calls: request.all_calls,
        definition_span: target.definition_span,
        parameter_list_span: target.parameter_container.span,
        call_spans,
        function_name: target.function_name,
        parameter_name: request.name,
        from_index,
        to_index: request.to_index,
        moved_arguments,
        rewritten,
        changed,
    })
}
