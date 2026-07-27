use anyhow::Result;

use super::super::RenameAtNamespace;
use super::scope::{enclosing_specialized_scope, occurrence_has_scope};
use super::{Candidate, SpecializedCandidateContext, push_candidate};
use crate::domain::rename::{
    RenameSymbolMacroRequest, plan_rename_symbol_macro, selection::apply_byte_span_edits,
};

pub(super) fn add(
    output: &mut Vec<Candidate>,
    context: &SpecializedCandidateContext<'_>,
) -> Result<()> {
    let plan = plan_rename_symbol_macro(RenameSymbolMacroRequest {
        input: context.input,
        dialect: context.dialect,
        from: context.from.clone(),
        to: context.to.clone(),
    })?;
    let scope = enclosing_specialized_scope(
        context.root_view,
        context.path,
        RenameAtNamespace::SymbolMacro,
    )?;
    let occurrences: Vec<_> = plan
        .definitions
        .iter()
        .chain(&plan.references)
        .filter(|item| {
            occurrence_has_scope(
                context.root_view,
                context.atom_paths,
                item.span,
                RenameAtNamespace::SymbolMacro,
                scope,
            )
        })
        .collect();
    let rewritten = apply_byte_span_edits(
        context.input,
        occurrences
            .iter()
            .map(|item| (item.span, item.replacement.clone()))
            .collect(),
    )?;
    push_candidate(
        output,
        RenameAtNamespace::SymbolMacro,
        context.selected_span,
        occurrences
            .iter()
            .filter(|item| plan.definitions.contains(item))
            .count()
            == 1,
        occurrences.iter().map(|item| item.span).collect(),
        rewritten,
    );
    Ok(())
}
