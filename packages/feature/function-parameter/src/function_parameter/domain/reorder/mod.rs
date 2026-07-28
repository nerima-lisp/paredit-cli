use paredit_core_edit::DocumentRefusal;

use crate::error::{CallSelectionError, FunctionParameterResult, ParameterSelectionError};

use paredit_core_edit::mutation_safety::reject_common_lisp_reader_conditionals;
use paredit_core_syntax::sexpr::SyntaxTree;

use super::calls::{
    FunctionCallPathRequest, reorder_function_parameter_call_edit, resolve_function_call_paths,
};
use super::definition::parse_reorder_function_parameters_definition;
use super::list_edit::{
    SpanEdit, apply_byte_span_edits, ensure_non_overlapping_spans, spans_overlap,
};
use super::types::{ReorderFunctionParametersPlan, ReorderFunctionParametersRequest};

pub use definition_edit::reorder_function_definition_edit;
pub use order::{
    build_new_relative_order, ensure_reorder_stays_within_parameter_groups, is_identity_order,
};
pub use parameter::{
    ParameterGroup, ReorderableParameter, ensure_parameter_is_reorderable, reorderable_parameters,
};

pub mod call_argument;
mod definition_edit;
mod order;
mod parameter;

pub fn plan_reorder_function_parameters(
    request: ReorderFunctionParametersRequest<'_>,
) -> FunctionParameterResult<ReorderFunctionParametersPlan> {
    if request.parameter_order.is_empty() {
        return Err(ParameterSelectionError::ReorderNeedsParameter.into());
    }

    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let target = parse_reorder_function_parameters_definition(
        request.dialect,
        &tree,
        &request.definition_path,
    )?;
    let reorderable_parameters =
        reorderable_parameters(&target.parameters, "reorder-function-parameters")?;
    let old_parameter_order = reorderable_parameters
        .iter()
        .map(|parameter| parameter.name.clone())
        .collect::<Vec<_>>();
    let new_relative_order =
        build_new_relative_order(&old_parameter_order, &request.parameter_order)?;
    ensure_reorder_stays_within_parameter_groups(
        &reorderable_parameters,
        &new_relative_order,
        "reorder-function-parameters",
    )?;
    let call_paths = resolve_function_call_paths(FunctionCallPathRequest {
        tree: &tree,
        dialect: request.dialect,
        explicit_call_paths: request.call_paths,
        all_calls: request.all_calls,
        definition_span: target.definition_span,
        definition_scope: target.definition_scope,
        function_name: &target.function_name,
        command: "reorder-function-parameters",
    })?;

    let mut edits = Vec::<SpanEdit>::with_capacity(call_paths.len() + 1);
    if !is_identity_order(&new_relative_order) {
        edits.push(reorder_function_definition_edit(
            request.input,
            &tree,
            &target.parameter_container,
            &reorderable_parameters,
            &new_relative_order,
            "reorder-function-parameters",
        )?);
    }

    let mut call_spans = Vec::with_capacity(call_paths.len());
    let mut reordered_arguments = Vec::with_capacity(call_paths.len());
    for call_path in &call_paths {
        let call_selection = tree.select_path(call_path)?;
        let call_view = call_selection.view();
        if spans_overlap(target.definition_span, call_selection.span()) {
            return Err(CallSelectionError::OverlapsDefinition {
                command: "reorder-function-parameters",
                path: call_path.to_string(),
            }
            .into());
        }
        let (call_span, reordered_argument, edit) = reorder_function_parameter_call_edit(
            request.input,
            &call_view,
            &target.function_name,
            target.call_argument_offset,
            &reorderable_parameters,
            &new_relative_order,
            "reorder-function-parameters",
        )?;
        call_spans.push(call_span);
        reordered_arguments.push(reordered_argument);
        if let Some(edit) = edit {
            edits.push(edit);
        }
    }

    let mut sorted_call_spans = call_spans.clone();
    sorted_call_spans.sort_by_key(|span| span.start());
    ensure_non_overlapping_spans(sorted_call_spans)?;
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "reorder-function-parameters",
            source,
        }
    })?;

    let changed = rewritten != request.input;
    Ok(ReorderFunctionParametersPlan {
        dialect: request.dialect,
        definition_path: request.definition_path,
        call_paths,
        all_calls: request.all_calls,
        definition_span: target.definition_span,
        parameter_list_span: target.parameter_container.span,
        call_spans,
        function_name: target.function_name,
        old_parameter_order,
        new_parameter_order: request.parameter_order,
        reordered_arguments,
        rewritten,
        changed,
    })
}
