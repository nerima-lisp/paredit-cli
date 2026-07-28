//! Use-case helpers for inlining function calls.

use paredit_core_edit::DocumentRefusal;

use crate::error::{InlineInternalError, InlineResult, InlineSafetyError};

use paredit_core_edit::mutation_safety::reject_common_lisp_reader_conditionals;
use paredit_core_semantics::lexical_scope::collect_unshadowed_symbol_references;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

mod calls;
mod definition;
mod macro_expansion;
mod rewrite;
mod substitution;
mod syntax;
#[cfg(test)]
mod tests;
mod types;

use calls::{
    bind_inline_function_arguments, parse_inline_function_call, resolve_function_call_paths,
};
use definition::{
    InlineDefinition, InlineDefinitionKind, InlineDestructurePattern, InlineParameter,
    InlineParameterBinding, InlineParameterKind, parse_inline_function_definition,
};
use macro_expansion::expand_inline_macro_body;
use rewrite::{apply_byte_span_edits, expand_definition_removal};
use substitution::substitute_inline_function_body;
use syntax::spans_overlap;
pub use types::{
    InlineCallSelection, InlineFunctionCallPlan, InlineFunctionParameterPlan, InlineFunctionPlan,
    InlineFunctionRequest,
};

#[derive(Debug)]
struct InlineFunctionParts {
    definition_span: ByteSpan,
    call_span: ByteSpan,
    function_name: SymbolName,
    parameters: Vec<InlineFunctionParameterPlan>,
    replacement: String,
}

pub fn plan_inline_function(
    request: InlineFunctionRequest<'_>,
) -> InlineResult<InlineFunctionPlan> {
    ensure_inline_function_dialect_supported(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let definition_selection = tree.select_path(&request.definition_path)?;
    let definition_span = definition_selection.span();
    // Inlining substitutes parsed parameters into the call site; it never
    // copies the definition's own comments there. Removing a definition that
    // has a comment (for example, documenting what the body does) would
    // silently discard it, since it is not preserved anywhere else.
    if request.remove_definition && tree.has_comment_in(definition_span) {
        return Err(InlineSafetyError::RemoveDefinitionWithComment.into());
    }
    let definition = parse_inline_function_definition(
        request.dialect,
        request.input,
        definition_selection.view(),
    )?;
    let function_name = definition.name.clone();
    let call_paths = resolve_function_call_paths(
        &tree,
        request.dialect,
        request.calls.explicit_paths(),
        request.calls.is_all_calls(),
        definition_span,
        &function_name,
        "inline-function",
    )?;

    let mut calls = Vec::with_capacity(call_paths.len());
    let mut edits = Vec::with_capacity(call_paths.len() + usize::from(request.remove_definition));
    for call_path in &call_paths {
        let definition_selection = tree.select_path(&request.definition_path)?;
        let call_selection = tree.select_path(call_path)?;
        let parts = inline_function_parts(
            request.dialect,
            request.input,
            definition_selection.view(),
            call_selection.view(),
            request.allow_duplicate_evaluation,
            request.allow_drop_arguments,
        )?;

        if parts.function_name != function_name {
            return Err(InlineInternalError::InconsistentFunctionName {
                expected: function_name.to_string(),
                found: parts.function_name.to_string(),
            }
            .into());
        }
        if spans_overlap(parts.definition_span, parts.call_span) {
            return Err(InlineSafetyError::DefinitionAndCallOverlap.into());
        }

        edits.push((parts.call_span, parts.replacement.clone()));
        calls.push(InlineFunctionCallPlan {
            call_path: call_path.clone(),
            call_span: parts.call_span,
            parameters: parts.parameters,
            replacement: parts.replacement,
        });
    }

    let call_spans = calls.iter().map(|call| call.call_span).collect::<Vec<_>>();
    let definition_removed = request.remove_definition;
    if definition_removed {
        edits.push((
            expand_definition_removal(request.input, definition_span),
            String::new(),
        ));
    }
    let rewritten = apply_byte_span_edits(request.input, edits)?;

    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "inline-function",
            source,
        }
    })?;

    Ok(InlineFunctionPlan {
        dialect: request.dialect,
        definition_path: request.definition_path,
        call_paths,
        all_calls: request.calls.is_all_calls(),
        definition_span,
        call_spans,
        function_name,
        calls,
        remove_definition: request.remove_definition,
        definition_removed,
        changed: rewritten != request.input,
        rewritten,
    })
}

#[must_use]
pub const fn supports_inline_function_dialect(dialect: Dialect) -> bool {
    matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp)
}

fn ensure_inline_function_dialect_supported(dialect: Dialect) -> InlineResult<()> {
    if supports_inline_function_dialect(dialect) {
        Ok(())
    } else {
        Err(InlineSafetyError::UnsupportedDialect {
            dialect: dialect.label().to_owned(),
        }
        .into())
    }
}

fn inline_function_parts(
    dialect: Dialect,
    input: &str,
    definition_selection: ExpressionView,
    call_selection: ExpressionView,
    allow_duplicate_evaluation: bool,
    allow_drop_arguments: bool,
) -> InlineResult<InlineFunctionParts> {
    let definition =
        parse_inline_function_definition(dialect, input, definition_selection.clone())?;
    validate_macro_environment_parameters(dialect, input, &definition)?;
    let function_name = definition.name.clone();
    let call = parse_inline_function_call(dialect, call_selection.clone(), &function_name, input)?;
    let bindings = bind_inline_function_arguments(
        dialect,
        &definition.params,
        call,
        &function_name,
        definition.accepts_other_keys,
        allow_drop_arguments,
    )?;
    let (replacement, parameters) = match definition.kind {
        InlineDefinitionKind::Function => {
            let (body_param_names, body_args): (Vec<_>, Vec<_>) =
                bindings.body_bindings.into_iter().unzip();
            let (intermediate_replacement, mut parameters) = substitute_inline_function_body(
                dialect,
                input,
                &definition.body,
                &body_param_names,
                &body_args,
                true,
                true,
            )?;
            let (argument_param_names, argument_args): (Vec<_>, Vec<_>) =
                bindings.argument_bindings.into_iter().unzip();
            let replacement_tree =
                SyntaxTree::parse_with_dialect(&intermediate_replacement, dialect)?;
            let replacement_body = replacement_view_for_inline_function(&replacement_tree)?;
            let (replacement, argument_parameters) = substitute_inline_function_body(
                dialect,
                &intermediate_replacement,
                &replacement_body,
                &argument_param_names,
                &argument_args,
                allow_duplicate_evaluation,
                allow_drop_arguments,
            )?;
            parameters.extend(argument_parameters);
            (
                wrap_inline_function_sequence(dialect, &definition.body, replacement),
                parameters,
            )
        }
        InlineDefinitionKind::Macro => expand_inline_macro_body(
            dialect,
            input,
            &definition.body,
            &bindings.body_bindings,
            &bindings.argument_bindings,
            allow_duplicate_evaluation,
            allow_drop_arguments,
        )?,
    };

    Ok(InlineFunctionParts {
        definition_span: definition_selection.span,
        call_span: call_selection.span,
        function_name,
        parameters,
        replacement,
    })
}

fn replacement_view_for_inline_function(tree: &SyntaxTree) -> InlineResult<ExpressionView> {
    if tree.root_children().len() == 1 {
        return Ok(tree.select_path(&Path::root_child(0))?.view());
    }
    Ok(tree.root_view())
}

fn wrap_inline_function_sequence(
    dialect: Dialect,
    body: &ExpressionView,
    replacement: String,
) -> String {
    if body.kind != ExpressionKind::Root {
        return replacement;
    }

    format!(
        "({} {})",
        dialect.inline_function_sequence_head(),
        replacement.trim()
    )
}

fn validate_macro_environment_parameters(
    dialect: Dialect,
    input: &str,
    definition: &InlineDefinition,
) -> InlineResult<()> {
    if definition.kind != InlineDefinitionKind::Macro {
        return Ok(());
    }

    let mut initialization_expressions = Vec::new();
    for param in &definition.params {
        collect_macro_initialization_expressions(param, &mut initialization_expressions);
    }

    for param in &definition.params {
        if !matches!(param.kind, InlineParameterKind::Environment) {
            continue;
        }
        let name = param
            .primary_name()
            .ok_or(InlineInternalError::EnvironmentNotSimple)?;

        reject_environment_references_in_expression(
            dialect,
            name,
            "macro body",
            input,
            &definition.body,
        )?;

        for (context, default_value) in &initialization_expressions {
            let default_tree =
                SyntaxTree::parse_with_dialect(default_value, dialect).map_err(|source| {
                    InlineInternalError::CouldNotParse {
                        context: context.to_string(),
                        value: default_value.to_string(),
                        source,
                    }
                })?;
            let default_expression = default_tree
                .select_path(&paredit_core_syntax::sexpr::Path::root_child(0))?
                .view();
            reject_environment_references_in_expression(
                dialect,
                name,
                context,
                default_value,
                &default_expression,
            )?;
        }
    }

    Ok(())
}

fn collect_macro_initialization_expressions<'a>(
    param: &'a InlineParameter,
    output: &mut Vec<(&'static str, &'a str)>,
) {
    if let Some(default_value) = param.default_value.as_deref() {
        let context = if matches!(param.kind, InlineParameterKind::Aux) {
            "&aux initializer"
        } else {
            "parameter default value"
        };
        output.push((context, default_value));
    }

    if let InlineParameterBinding::Destructure(pattern) = &param.binding {
        collect_macro_destructure_initialization_expressions(pattern, output);
    }
}

fn collect_macro_destructure_initialization_expressions<'a>(
    pattern: &'a InlineDestructurePattern,
    output: &mut Vec<(&'static str, &'a str)>,
) {
    match pattern {
        InlineDestructurePattern::Name(_) => {}
        InlineDestructurePattern::List(items) => {
            for item in &items.required {
                collect_macro_destructure_initialization_expressions(item, output);
            }
            for item in &items.optional {
                if let Some(default_value) = item.default_value.as_deref() {
                    output.push(("nested &optional default value", default_value));
                }
                collect_macro_destructure_initialization_expressions(&item.binding, output);
            }
            if let Some(rest) = &items.rest {
                collect_macro_destructure_initialization_expressions(rest, output);
            }
            for item in &items.keys {
                if let Some(default_value) = item.default_value.as_deref() {
                    output.push(("nested &key default value", default_value));
                }
                collect_macro_destructure_initialization_expressions(&item.binding, output);
            }
            for item in &items.aux {
                collect_macro_initialization_expressions(item, output);
            }
        }
    }
}

fn reject_environment_references_in_expression(
    dialect: Dialect,
    parameter_name: &str,
    context: &str,
    input: &str,
    expression: &ExpressionView,
) -> InlineResult<()> {
    let symbol = SymbolName::new(parameter_name.to_owned())?;
    let mut spans = Vec::new();
    collect_unshadowed_symbol_references(dialect, expression, &symbol, input, &mut spans);
    if spans.is_empty() {
        return Ok(());
    }

    Err(InlineSafetyError::ReferencesEnvironment {
        parameter: parameter_name.to_string(),
        context: context.to_string(),
    }
    .into())
}
