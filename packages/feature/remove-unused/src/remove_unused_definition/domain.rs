use anyhow::{Context, Result};

use crate::domain::dialect::Dialect;
use crate::domain::mutation_safety::reject_overlapping_common_lisp_reader_time_forms;
use crate::domain::sexpr::SyntaxTree;

mod candidates;
mod policy;
mod rewrite;
#[cfg(test)]
mod tests;
mod types;

use candidates::collect_unused_definition_candidates;
use policy::{collect_exported_symbol_index, definition_is_bulk_removable, definition_is_exported};
use rewrite::{expand_definition_removal, replace_span};

pub use types::{
    PlannedDefinitionRemoval, RemoveUnusedDefinitionInputFile, RemoveUnusedDefinitionsFilePlan,
    RemoveUnusedDefinitionsPlan, RemoveUnusedDefinitionsRequest, SkippedDefinitionRemoval,
    SkippedDefinitionRemovalReason, UnusedDefinitionDefinition,
};

pub fn plan_remove_unused_definitions(
    request: RemoveUnusedDefinitionsRequest,
) -> Result<RemoveUnusedDefinitionsPlan> {
    for file in &request.files {
        if file.dialect == Dialect::Unknown {
            anyhow::bail!(
                "remove-unused-definition does not support dialect unknown: {}",
                file.path.display()
            );
        }
    }

    let exported_symbols = collect_exported_symbol_index(&request.package_definitions);
    let unused_reports = collect_unused_definition_candidates(&request.files)?;
    let mut files = Vec::with_capacity(request.files.len());

    for (file, report) in request.files.iter().zip(&unused_reports) {
        files.push(plan_file_removals(
            file,
            report,
            &request,
            &exported_symbols,
        )?);
    }

    let candidate_count = unused_reports
        .iter()
        .flat_map(|report| &report.definitions)
        .filter(|item| item.references.is_empty())
        .count();
    let removal_count = files.iter().map(|file| file.removals.len()).sum();
    let skipped_count = files.iter().map(|file| file.skipped.len()).sum();
    let changed = files.iter().any(|file| file.changed);

    Ok(RemoveUnusedDefinitionsPlan {
        files,
        candidate_count,
        removal_count,
        skipped_count,
        changed,
    })
}

fn plan_file_removals(
    file: &RemoveUnusedDefinitionInputFile,
    report: &candidates::UnusedDefinitionFile,
    request: &RemoveUnusedDefinitionsRequest,
    exported_symbols: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> Result<RemoveUnusedDefinitionsFilePlan> {
    let mut removals = Vec::new();
    let mut skipped = Vec::new();
    let empty_exported_symbols = std::collections::HashMap::new();
    let exported_symbols = if file.dialect == Dialect::CommonLisp {
        exported_symbols
    } else {
        &empty_exported_symbols
    };

    for item in report
        .definitions
        .iter()
        .filter(|item| item.references.is_empty())
    {
        match classify_definition_action(file, item, request, exported_symbols) {
            DefinitionAction::Remove(removal) => removals.push(removal),
            DefinitionAction::Skip(skipped_removal) => skipped.push(skipped_removal),
        }
    }

    let rewritten = rewrite_file_without_unused_definitions(file, &mut removals)?;
    let changed = rewritten != file.text;

    Ok(RemoveUnusedDefinitionsFilePlan {
        path: file.path.clone(),
        dialect: file.dialect,
        package: file.package.clone(),
        rewritten,
        changed,
        removals,
        skipped,
    })
}

enum DefinitionAction {
    Remove(PlannedDefinitionRemoval),
    Skip(SkippedDefinitionRemoval),
}

fn classify_definition_action(
    file: &RemoveUnusedDefinitionInputFile,
    item: &candidates::UnusedDefinitionItem,
    request: &RemoveUnusedDefinitionsRequest,
    exported_symbols: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> DefinitionAction {
    if !request.include_exported && definition_is_exported(&item.definition, exported_symbols) {
        return DefinitionAction::Skip(SkippedDefinitionRemoval {
            definition: item.definition.clone(),
            reason: SkippedDefinitionRemovalReason::ExportedDefinition,
        });
    }

    if request.include_protected || definition_is_bulk_removable(item.definition.category) {
        return DefinitionAction::Remove(PlannedDefinitionRemoval {
            definition: item.definition.clone(),
            definition_text: item.definition.span.slice(&file.text).to_owned(),
            removal_span: item.definition.span,
        });
    }

    DefinitionAction::Skip(SkippedDefinitionRemoval {
        definition: item.definition.clone(),
        reason: SkippedDefinitionRemovalReason::ProtectedDefinitionCategory,
    })
}

fn rewrite_file_without_unused_definitions(
    file: &RemoveUnusedDefinitionInputFile,
    removals: &mut [PlannedDefinitionRemoval],
) -> Result<String> {
    removals.sort_by(|left, right| {
        right
            .definition
            .span
            .start()
            .get()
            .cmp(&left.definition.span.start().get())
    });

    let mut rewritten = file.text.clone();
    for removal in removals {
        let expanded = expand_definition_removal(&rewritten, removal.definition.span);
        removal.removal_span = expanded;
        let tree = SyntaxTree::parse_with_dialect(&rewritten, file.dialect).with_context(|| {
            format!(
                "file would become invalid before removing unused definitions: {}",
                file.path.display()
            )
        })?;
        reject_overlapping_common_lisp_reader_time_forms(&tree, file.dialect, [expanded])?;
        rewritten = replace_span(&rewritten, expanded, "");
    }

    SyntaxTree::parse_with_dialect(&rewritten, file.dialect).with_context(|| {
        format!(
            "file would become invalid after removing unused definitions: {}",
            file.path.display()
        )
    })?;

    Ok(rewritten)
}
