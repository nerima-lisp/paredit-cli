use paredit_core_edit::DocumentRefusal;

use crate::error::{
    CallSelectionError, FunctionParameterError, FunctionParameterResult, LambdaListError,
    ListEditError,
};
use paredit_core_syntax::dialect::Dialect;

use paredit_core_edit::mutation_safety::reject_common_lisp_reader_conditionals;
use paredit_core_syntax::sexpr::SyntaxTree;

use super::calls::{
    FunctionCallPathRequest, add_function_parameter_call_edit, resolve_function_call_paths,
};
use super::definition::parse_add_function_parameter_definition;
use super::list_edit::{apply_byte_span_edits, ensure_non_overlapping_spans, spans_overlap};
use super::types::{
    AddFunctionParameterPlan, AddFunctionParameterRequest, FunctionParameterSection,
};

mod definition_insertion;

use definition_insertion::{DefinitionInsertionPlan, resolve_definition_insertion_plan};

pub fn plan_add_function_parameter(
    request: AddFunctionParameterRequest<'_>,
) -> FunctionParameterResult<AddFunctionParameterPlan> {
    let argument = validate_argument(&request.argument, request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let target = parse_add_function_parameter_definition(
        request.dialect,
        &tree,
        &request.definition_path,
        &request.name,
    )?;
    let insertion_plan = resolve_definition_insertion_plan(&target, &request);
    if target.has_lambda_list_marker && insertion_plan.is_none() {
        return Err(LambdaListError::AddOnlyFlatOrMarkers.into());
    }
    let resolved_section = insertion_plan
        .as_ref()
        .map(DefinitionInsertionPlan::resolved_section)
        .unwrap_or(FunctionParameterSection::Positional);
    let call_paths = resolve_function_call_paths(FunctionCallPathRequest {
        tree: &tree,
        dialect: request.dialect,
        explicit_call_paths: request.call_paths.clone(),
        all_calls: request.all_calls,
        definition_span: target.definition_span,
        definition_scope: target.definition_scope,
        function_name: &target.function_name,
        command: "add-function-parameter",
    })?;

    let mut edits = Vec::with_capacity(call_paths.len() + 1);
    if let Some(insertion_plan) = insertion_plan.as_ref() {
        edits.push(insertion_plan.definition_edit(&target, &request)?);
    } else {
        edits.push(super::list_edit::insertion_edit_for_list_item(
            &target.parameter_container,
            target.protected_prefix_count,
            request.name.as_str(),
            request.insert,
        )?);
    }

    let mut call_spans = Vec::with_capacity(call_paths.len());
    for call_path in &call_paths {
        let call_selection = tree.select_path(call_path)?;
        let call_view = call_selection.view();
        if spans_overlap(target.definition_span, call_selection.span()) {
            return Err(CallSelectionError::OverlapsDefinition {
                command: "add-function-parameter",
                path: call_path.to_string(),
            }
            .into());
        }
        let edit = if let Some(insertion_plan) = insertion_plan.as_ref() {
            insertion_plan.call_edit(
                &call_view,
                &target.function_name,
                target.call_argument_offset,
                &argument,
                request.insert,
            )?
        } else {
            add_function_parameter_call_edit(
                &call_view,
                &target.function_name,
                target.call_argument_offset,
                &argument,
                request.insert,
            )?
        };
        call_spans.push(call_selection.span());
        edits.push(edit);
    }

    let mut sorted_call_spans = call_spans.clone();
    sorted_call_spans.sort_by_key(|span| span.start());
    ensure_non_overlapping_spans(sorted_call_spans)?;
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "add-function-parameter",
            source,
        }
    })?;

    let changed = rewritten != request.input;
    Ok(AddFunctionParameterPlan {
        dialect: request.dialect,
        definition_path: request.definition_path,
        call_paths,
        all_calls: request.all_calls,
        definition_span: target.definition_span,
        parameter_list_span: target.parameter_container.span,
        call_spans,
        function_name: target.function_name,
        parameter_name: request.name,
        argument,
        insert: request.insert,
        section: resolved_section,
        rewritten,
        changed,
    })
}

fn validate_argument(argument: &str, dialect: Dialect) -> FunctionParameterResult<String> {
    let argument = argument.trim().to_owned();
    if argument.is_empty() {
        return Err(ListEditError::ArgumentEmpty.into());
    }
    let argument_tree = SyntaxTree::parse_with_dialect(&argument, dialect)
        .map_err(|source| FunctionParameterError::ArgumentDoesNotParse { source })?;
    if argument_tree.root_children().len() != 1 {
        return Err(ListEditError::ArgumentNotOneForm.into());
    }
    Ok(argument)
}
