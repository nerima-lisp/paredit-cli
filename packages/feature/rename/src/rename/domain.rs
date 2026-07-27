//! Use-case helpers for renaming callable definitions and transforming call sites.

mod at;
mod binding;
mod call_identity;
mod function;
mod macrolet;
mod reader;
mod replace_call;
mod selection;
mod symbol_macro;
#[cfg(test)]
mod tests;
mod types;
mod unwrap;
mod wrap;

use paredit_core_edit::DocumentRefusal;

use crate::error::{RenameError, RenameResult};

use paredit_core_edit::mutation_safety::ReaderConditionalSafetyError;
use paredit_core_syntax::sexpr::SyntaxTree;

use binding::binding_rename_parts;
use selection::{apply_byte_span_edits, collect_symbol_atom_spans, select_rename_target};

pub type RenameReaderSafetyError = ReaderConditionalSafetyError;

pub use crate::rename_types::domain::{
    FunctionCallScope, ReplaceFunctionCallsScope, UnwrapFunctionCallsScope, WrapFunctionCallsScope,
};

pub use at::supports_rename_at_dialect;
pub use at::{RenameAtError, RenameAtNamespace, RenameAtPlan, RenameAtRequest, plan_rename_at};
pub use function::{collect_callable_definition_renames, collect_function_call_head_renames};
pub use macrolet::{
    collect_local_function_binding_renames, collect_local_function_call_head_renames,
    collect_macrolet_binding_renames, collect_macrolet_call_head_renames,
};
pub use replace_call::{
    ReplaceFunctionCallSite, ReplaceFunctionCallsPlan, ReplaceFunctionCallsRequest,
    plan_replace_function_calls,
};
pub use symbol_macro::{
    collect_define_symbol_macro_definition_renames, collect_define_symbol_macro_reference_renames,
};
pub use types::{
    RenameBindingPlan, RenameBindingRequest, RenameFunctionOccurrence, RenameFunctionPlan,
    RenameFunctionRequest, RenameInFormPlan, RenameInFormRequest, RenameLocalFunctionPlan,
    RenameLocalFunctionRequest, RenameMacroletPlan, RenameMacroletRequest, RenameSymbolMacroPlan,
    RenameSymbolMacroRequest, RenameTarget,
};
pub use unwrap::{
    UnwrapFunctionCallSite, UnwrapFunctionCallsPlan, UnwrapFunctionCallsRequest,
    plan_unwrap_function_calls,
};
pub use wrap::{
    WrapFunctionCallSite, WrapFunctionCallsPlan, WrapFunctionCallsRequest, plan_wrap_function_calls,
};

pub fn plan_rename_function(
    request: RenameFunctionRequest<'_>,
) -> RenameResult<RenameFunctionPlan> {
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .map_err(|source| DocumentRefusal::InputParseFailed { source })?;
    let definitions =
        collect_callable_definition_renames(&tree, request.dialect, &request.from, &request.to)?;
    let calls =
        collect_function_call_head_renames(&tree, request.dialect, &request.from, &request.to)?;
    let edits = definitions
        .iter()
        .chain(calls.iter())
        .map(|occurrence| (occurrence.span, occurrence.replacement.clone()))
        .collect::<Vec<_>>();
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "renamed",
            source,
        }
    })?;

    Ok(RenameFunctionPlan {
        dialect: request.dialect,
        definitions,
        calls,
        changed: rewritten != request.input,
        rewritten,
    })
}

pub fn plan_rename_macrolet(
    request: RenameMacroletRequest<'_>,
) -> RenameResult<RenameMacroletPlan> {
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .map_err(|source| DocumentRefusal::InputParseFailed { source })?;
    let definitions =
        collect_macrolet_binding_renames(&tree, request.dialect, &request.from, &request.to)?;
    let calls =
        collect_macrolet_call_head_renames(&tree, request.dialect, &request.from, &request.to)?;
    let edits = definitions
        .iter()
        .chain(calls.iter())
        .map(|occurrence| (occurrence.span, occurrence.replacement.clone()))
        .collect::<Vec<_>>();
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "renamed",
            source,
        }
    })?;

    Ok(RenameMacroletPlan {
        dialect: request.dialect,
        definitions,
        calls,
        changed: rewritten != request.input,
        rewritten,
    })
}

pub fn plan_rename_symbol_macro(
    request: RenameSymbolMacroRequest<'_>,
) -> RenameResult<RenameSymbolMacroPlan> {
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .map_err(|source| DocumentRefusal::InputParseFailed { source })?;
    let definitions = collect_define_symbol_macro_definition_renames(
        &tree,
        request.dialect,
        &request.from,
        &request.to,
    )?;
    let references = collect_define_symbol_macro_reference_renames(
        &tree,
        request.dialect,
        &request.from,
        &request.to,
    )?;
    let edits = definitions
        .iter()
        .chain(references.iter())
        .map(|occurrence| (occurrence.span, occurrence.replacement.clone()))
        .collect::<Vec<_>>();
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "renamed",
            source,
        }
    })?;

    Ok(RenameSymbolMacroPlan {
        dialect: request.dialect,
        definitions,
        references,
        changed: rewritten != request.input,
        rewritten,
    })
}

pub fn plan_rename_local_function(
    request: RenameLocalFunctionRequest<'_>,
) -> RenameResult<RenameLocalFunctionPlan> {
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .map_err(|source| DocumentRefusal::InputParseFailed { source })?;
    let definitions =
        collect_local_function_binding_renames(&tree, request.dialect, &request.from, &request.to)?;
    let calls = collect_local_function_call_head_renames(
        &tree,
        request.dialect,
        &request.from,
        &request.to,
    )?;
    let edits = definitions
        .iter()
        .chain(calls.iter())
        .map(|occurrence| (occurrence.span, occurrence.replacement.clone()))
        .collect::<Vec<_>>();
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "renamed",
            source,
        }
    })?;

    Ok(RenameLocalFunctionPlan {
        dialect: request.dialect,
        definitions,
        calls,
        changed: rewritten != request.input,
        rewritten,
    })
}

pub fn plan_rename_in_form(request: RenameInFormRequest<'_>) -> RenameResult<RenameInFormPlan> {
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .map_err(|source| DocumentRefusal::InputParseFailed { source })?;
    let path = match &request.target {
        RenameTarget::Path(path) => Some(path.clone()),
        RenameTarget::Offset(_) => None,
    };
    let view = select_rename_target(&tree, &request.target)?.view();
    let mut occurrences = Vec::new();
    collect_symbol_atom_spans(&view, &request.from, &mut occurrences);
    occurrences.sort_by_key(|span| span.start());

    let edits = occurrences
        .iter()
        .map(|span| (*span, request.to.as_str().to_owned()))
        .collect::<Vec<_>>();
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "renamed",
            source,
        }
    })?;

    Ok(RenameInFormPlan {
        dialect: request.dialect,
        path,
        scope_span: view.span,
        from: request.from,
        to: request.to,
        occurrences,
        changed: rewritten != request.input,
        rewritten,
    })
}

pub fn plan_rename_binding(request: RenameBindingRequest<'_>) -> RenameResult<RenameBindingPlan> {
    let semantic = request
        .dialect
        .verify_rename_binding()
        .map_err(|source| RenameError::BindingRenameUnsupportedDialect { source })?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .map_err(|source| DocumentRefusal::InputParseFailed { source })?;
    let path = match &request.target {
        RenameTarget::Path(path) => Some(path.clone()),
        RenameTarget::Offset(_) => None,
    };
    let view = select_rename_target(&tree, &request.target)?.view();
    let parts = binding_rename_parts(semantic, &view, &request.from, request.input)?;

    let mut edits = Vec::with_capacity(parts.reference_spans.len() + 1);
    edits.push((
        parts.binding_edit.span,
        parts.binding_edit.replacement(request.input, &request.to),
    ));
    edits.extend(
        parts
            .reference_spans
            .iter()
            .map(|span| (*span, request.to.as_str().to_owned())),
    );
    let rewritten = apply_byte_span_edits(request.input, edits)?;
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "renamed",
            source,
        }
    })?;

    Ok(RenameBindingPlan {
        dialect: request.dialect,
        path,
        form: parts.form,
        form_span: parts.form_span,
        binding_span: parts.binding_span,
        from: request.from,
        to: request.to,
        references: parts.reference_spans,
        shadowed_scope_count: parts.shadowed_scope_count,
        changed: rewritten != request.input,
        rewritten,
    })
}
