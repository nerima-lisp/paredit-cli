//! Use-case helpers for extracting functions from selected expressions.

use paredit_core_edit::DocumentRefusal;

use crate::error::{ExtractionResult, ExtractionTargetError};

use paredit_core_edit::extract_shared::{insert_top_level_form, replace_span_checked};
use paredit_core_edit::mutation_safety::reject_common_lisp_reader_conditionals;
use paredit_core_syntax::sexpr::{ExpressionView, SyntaxTree};

mod inference;
pub mod rewrite;
mod syntax;
#[cfg(test)]
mod tests;
mod types;

use rewrite::{extracted_call, extracted_definition};

pub use types::{ExtractFunctionInsert, ExtractFunctionPlan, ExtractFunctionRequest};

pub fn plan_extract_function(
    request: ExtractFunctionRequest<'_>,
) -> ExtractionResult<ExtractFunctionPlan> {
    let semantic = request
        .dialect
        .verify_extract_function()
        .map_err(|source| ExtractionTargetError::DialectDoesNotSupportExtractFunction { source })?;
    request.selection.validate_source(request.input)?;
    let input_tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputNotAnSexprDocument {
                operation: "extract-function",
                source,
            }
        })?;
    reject_common_lisp_reader_conditionals(&input_tree, request.dialect)?;

    let span = request.selection.span();
    let selected = request.selection.text().to_owned();
    let mut params = request.explicit_params;
    let inferred_params = if request.infer_params {
        inference::infer_extract_function_params(semantic, &request.selection.view(), &params)
    } else {
        Vec::new()
    };
    for param in &inferred_params {
        if !params
            .iter()
            .any(|existing| inference::extract_function_param_name_eq(semantic, existing, param))
        {
            params.push(param.clone());
        }
    }

    let call = extracted_call(&request.name, &params);
    let definition = extracted_definition(request.dialect, &request.name, &params, &selected);
    let replaced = replace_span_checked(request.input, span, &call)?;
    let replaced_tree =
        SyntaxTree::parse_with_dialect(&replaced, request.dialect).map_err(|source| {
            DocumentRefusal::OutputNotAnSexprDocument {
                operation: "replacement",
                source,
            }
        })?;
    let (rewritten, anchor_span) = insert_top_level_form(
        &replaced,
        &replaced_tree,
        &definition,
        request.insert,
        request.anchor_path.as_ref(),
        "extract-function --anchor-path",
    )?;

    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "extracted",
            source,
        }
    })?;

    Ok(ExtractFunctionPlan {
        dialect: request.dialect,
        path: request.path,
        span_start: span.start().get(),
        span_end: span.end().get(),
        name: request.name,
        params,
        inferred_params,
        insert: request.insert,
        anchor_path: request.anchor_path,
        anchor_span,
        call,
        definition,
        changed: rewritten != request.input,
        rewritten,
    })
}

#[must_use]
pub fn infer_extract_function_params(
    dialect: paredit_core_syntax::dialect::Dialect,
    selection: &ExpressionView,
    explicit_params: &[String],
) -> Vec<String> {
    let Ok(semantic) = dialect.verify_extract_function() else {
        return Vec::new();
    };
    inference::infer_extract_function_params(semantic, selection, explicit_params)
}
