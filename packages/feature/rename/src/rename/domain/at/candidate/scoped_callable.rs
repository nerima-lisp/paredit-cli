use anyhow::Result;

use super::super::RenameAtNamespace;
use super::scope::{enclosing_specialized_scope, occurrence_has_scope};
use super::{Candidate, SpecializedCandidateContext, push_candidate};
use crate::domain::rename::{
    RenameFunctionOccurrence, RenameLocalFunctionRequest, RenameMacroletRequest,
    plan_rename_local_function, plan_rename_macrolet, selection::apply_byte_span_edits,
};
use crate::domain::sexpr::ByteSpan;

pub(super) fn add_local_function(
    output: &mut Vec<Candidate>,
    context: &SpecializedCandidateContext<'_>,
) -> Result<()> {
    let plan = plan_rename_local_function(RenameLocalFunctionRequest {
        input: context.input,
        dialect: context.dialect,
        from: context.from.clone(),
        to: context.to.clone(),
    })?;
    add_scoped(
        output,
        context,
        RenameAtNamespace::LocalFunction,
        &plan.definitions,
        &plan.calls,
    )
}

pub(super) fn add_macro(
    output: &mut Vec<Candidate>,
    context: &SpecializedCandidateContext<'_>,
) -> Result<()> {
    let plan = plan_rename_macrolet(RenameMacroletRequest {
        input: context.input,
        dialect: context.dialect,
        from: context.from.clone(),
        to: context.to.clone(),
    })?;
    add_scoped(
        output,
        context,
        RenameAtNamespace::Macro,
        &plan.definitions,
        &plan.calls,
    )
}

fn add_scoped(
    output: &mut Vec<Candidate>,
    context: &SpecializedCandidateContext<'_>,
    namespace: RenameAtNamespace,
    definitions: &[RenameFunctionOccurrence],
    calls: &[RenameFunctionOccurrence],
) -> Result<()> {
    let scope = enclosing_specialized_scope(context.root_view, context.path, namespace)?;
    let occurrences: Vec<_> = definitions
        .iter()
        .chain(calls)
        .filter(|item| {
            occurrence_has_scope(
                context.root_view,
                context.atom_paths,
                item.span,
                namespace,
                scope,
            )
        })
        .collect();
    let spans: Vec<ByteSpan> = occurrences.iter().map(|item| item.span).collect();
    let rewritten = apply_byte_span_edits(
        context.input,
        occurrences
            .iter()
            .map(|item| (item.span, item.replacement.clone()))
            .collect(),
    )?;
    push_candidate(
        output,
        namespace,
        context.selected_span,
        occurrences
            .iter()
            .filter(|item| definitions.contains(item))
            .count()
            == 1,
        spans,
        rewritten,
    );
    Ok(())
}
