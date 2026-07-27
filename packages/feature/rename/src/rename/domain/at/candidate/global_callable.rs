use anyhow::Result;

use super::super::RenameAtNamespace;
use super::super::safety::ensure_function_occurrences_are_unqualified;
use super::{Candidate, SpecializedCandidateContext, push_candidate};
use crate::domain::common_lisp::CommonLispOperator;
use crate::domain::definition::definition_shape;
use crate::domain::dialect::Dialect;
use crate::domain::rename::reader::executable_reader_context_at_path;
use crate::domain::rename::selection::{apply_byte_span_edits, list_head};
use crate::domain::rename::{
    RenameFunctionOccurrence, RenameFunctionRequest, plan_rename_function,
};
use crate::domain::sexpr::Path;

pub(super) fn add(
    output: &mut Vec<Candidate>,
    context: &SpecializedCandidateContext<'_>,
) -> Result<()> {
    if output
        .iter()
        .any(|candidate| candidate.namespace == RenameAtNamespace::Value)
    {
        return Ok(());
    }
    let plan = plan_rename_function(RenameFunctionRequest {
        input: context.input,
        dialect: context.dialect,
        from: context.from.clone(),
        to: context.to.clone(),
    })?;
    ensure_function_occurrences_are_unqualified(&plan.definitions, &plan.calls)?;
    let occurrences = executable_occurrences(context, &plan.definitions, &plan.calls)?;
    let rewritten = apply_byte_span_edits(
        context.input,
        occurrences
            .iter()
            .map(|occurrence| (occurrence.span, occurrence.replacement.clone()))
            .collect(),
    )?;
    push_candidate(
        output,
        namespace_for_selected_definition(context)?,
        context.selected_span,
        true,
        occurrences
            .into_iter()
            .map(|occurrence| occurrence.span)
            .collect(),
        rewritten,
    );
    Ok(())
}

fn executable_occurrences<'a>(
    context: &SpecializedCandidateContext<'_>,
    definitions: &'a [RenameFunctionOccurrence],
    calls: &'a [RenameFunctionOccurrence],
) -> Result<Vec<&'a RenameFunctionOccurrence>> {
    let mut executable = Vec::new();
    for occurrence in definitions.iter().chain(calls) {
        let Some(path) = context.atom_paths.path_for_span(occurrence.span) else {
            continue;
        };
        if executable_reader_context_at_path(context.tree, context.dialect, &path)? {
            executable.push(occurrence);
        }
    }
    Ok(executable)
}

/// Whether a definition introduces a globally callable Common Lisp macro
/// (`defmacro`/`define-compiler-macro`), as opposed to a setf-expander
/// definition (`defsetf`/`define-setf-expander`) which returns code for an
/// existing place rather than naming a new callable.
fn is_global_macro_definition(dialect: Dialect, head: &str) -> bool {
    matches!(dialect, Dialect::CommonLisp | Dialect::Unknown)
        && matches!(
            CommonLispOperator::from_head(head),
            Some(CommonLispOperator::Defmacro | CommonLispOperator::DefineCompilerMacro)
        )
}

fn namespace_for_selected_definition(
    context: &SpecializedCandidateContext<'_>,
) -> Result<RenameAtNamespace> {
    for (index, _) in context.tree.root_children().iter().enumerate() {
        let form_path = Path::root_child(index);
        let view = context.tree.select_path(&form_path)?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        if !is_global_macro_definition(context.dialect, head) {
            continue;
        }
        let Some(name_target) = definition_shape(context.dialect, &view, head)
            .and_then(|shape| shape.name_target(&view, &form_path))
        else {
            continue;
        };
        if name_target.span == context.selected_span {
            return Ok(RenameAtNamespace::GlobalMacro);
        }
    }
    Ok(RenameAtNamespace::Function)
}
