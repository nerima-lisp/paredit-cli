use anyhow::{Context, Result};

use crate::domain::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::sexpr::SyntaxTree;

use super::calls::{FunctionCallPathRequest, resolve_function_call_paths};
use super::definition::parse_remove_function_parameter_definition;
use super::list_edit::{apply_byte_span_edits, ensure_non_overlapping_spans, spans_overlap};
use super::types::{RemoveFunctionParameterPlan, RemoveFunctionParameterRequest};

mod call_edit;
mod metadata;

use call_edit::remove_call_argument_edit;
use metadata::resolve_remove_parameter_metadata;

pub fn plan_remove_function_parameter(
    request: RemoveFunctionParameterRequest<'_>,
) -> Result<RemoveFunctionParameterPlan> {
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let target = parse_remove_function_parameter_definition(
        request.dialect,
        &tree,
        &request.definition_path,
    )?;
    let parameter = resolve_remove_parameter_metadata(&target, &request)?;
    let call_paths = resolve_function_call_paths(FunctionCallPathRequest {
        tree: &tree,
        dialect: request.dialect,
        explicit_call_paths: request.call_paths,
        all_calls: request.all_calls,
        definition_span: target.definition_span,
        definition_scope: target.definition_scope,
        function_name: &target.function_name,
        command: "remove-function-parameter",
    })?;

    let mut edits = Vec::with_capacity(call_paths.len() + 1);
    edits.push(parameter.definition_edit.clone());

    let mut call_spans = Vec::with_capacity(call_paths.len());
    let mut removed_arguments = Vec::with_capacity(call_paths.len());
    for call_path in &call_paths {
        let call_selection = tree.select_path(call_path)?;
        if spans_overlap(target.definition_span, call_selection.span()) {
            anyhow::bail!(
                "remove-function-parameter call path {call_path} overlaps the selected definition"
            );
        }
        let call_edit = remove_call_argument_edit(
            request.input,
            &call_selection.view(),
            &target.function_name,
            target.call_argument_offset,
            &parameter,
            request.missing_argument_policy,
        )?;
        call_spans.push(call_edit.span);
        removed_arguments.push(call_edit.removed_argument);
        if let Some(edit) = call_edit.edit {
            edits.push(edit);
        }
    }

    let mut sorted_call_spans = call_spans.clone();
    sorted_call_spans.sort_by_key(|span| span.start());
    ensure_non_overlapping_spans(sorted_call_spans)?;
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("remove-function-parameter output is not a valid S-expression document")?;

    let changed = rewritten != request.input;
    Ok(RemoveFunctionParameterPlan {
        dialect: request.dialect,
        definition_path: request.definition_path,
        call_paths,
        all_calls: request.all_calls,
        definition_span: target.definition_span,
        parameter_list_span: target.parameter_container.span,
        call_spans,
        function_name: target.function_name,
        parameter_name: request.name,
        parameter_index: parameter.parameter_index,
        parameter_keyword: parameter.parameter_keyword,
        removed_arguments,
        rewritten,
        changed,
    })
}
