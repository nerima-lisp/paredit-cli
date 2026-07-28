use super::super::super::args::RefactorPreviewMode;
use super::super::super::types::plan::WorkspaceRefactorPlanDiscovery;
use super::super::super::types::preview::{RefactorPreview, RefactorPreviewFile};
use crate::refactor::usecase::preview::{
    RefactorPreviewPolicyOptions as DomainRefactorPreviewPolicyOptions, RefactorPreviewSummary,
    evaluate_refactor_preview_policy, refactor_preview_edits,
};
use anyhow::Result;
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::shared::apply_byte_span_edits;
use paredit_core_cli::shared::bounded_preview;
use paredit_core_cli::shared::matching_symbol_occurrences;
use paredit_core_cli::shared::read_input_dialect_and_tree;
use paredit_core_cli::shared::stable_text_hash;
use paredit_core_syntax::sexpr::SymbolName;
use paredit_core_syntax::sexpr::SyntaxTree;
use paredit_feature_rename::rename::cli as rename;
use std::path::PathBuf;

// Public since the extraction: crate-internal visibility cannot cross a
// crate boundary, so this lint applies for the first time.
#[derive(Debug)]
pub struct BuildRefactorPreviewRequest<'a> {
    pub paths: &'a [PathBuf],
    pub dialect: Option<DialectArg>,
    pub from: &'a SymbolName,
    pub to: &'a SymbolName,
    pub mode: RefactorPreviewMode,
    pub max_preview_bytes: usize,
    pub write: bool,
    pub policy_options: DomainRefactorPreviewPolicyOptions,
    pub workspace: Option<WorkspaceRefactorPlanDiscovery>,
}

pub fn build_refactor_preview(request: BuildRefactorPreviewRequest<'_>) -> Result<RefactorPreview> {
    let mut files = Vec::with_capacity(request.paths.len());
    let mut total_definitions = 0usize;
    let mut total_target_occurrences = 0usize;

    for file in request.paths {
        let (input, dialect, tree) =
            read_input_dialect_and_tree(Some(file.clone()), request.dialect)?;
        total_target_occurrences += matching_symbol_occurrences(&tree, request.to).len();
        let (rewritten, edits, definition_count) = match request.mode {
            RefactorPreviewMode::Symbol => {
                let raw_edits = matching_symbol_occurrences(&tree, request.from)
                    .into_iter()
                    .map(|occurrence| (occurrence.span, request.to.as_str().to_owned()))
                    .collect::<Vec<_>>();
                let rewritten = apply_byte_span_edits(&input.text, raw_edits.clone())?;
                (rewritten, refactor_preview_edits(&raw_edits), 0)
            }
            RefactorPreviewMode::Function => {
                let definitions = rename::shared::collect_callable_definition_renames(
                    &tree,
                    dialect,
                    request.from,
                    request.to,
                )?;
                let calls = rename::shared::collect_function_call_head_renames(
                    &tree,
                    dialect,
                    request.from,
                    request.to,
                )?;
                let raw_edits = definitions
                    .iter()
                    .chain(calls.iter())
                    .map(|edit| (edit.span, edit.replacement.clone()))
                    .collect::<Vec<_>>();
                let rewrite = apply_byte_span_edits(&input.text, raw_edits.clone())?;
                let definition_count = definitions.len();
                (
                    rewrite,
                    refactor_preview_edits(&raw_edits),
                    definition_count,
                )
            }
        };
        total_definitions += definition_count;

        let changed = rewritten != input.text;
        let output_parse_ok =
            !changed || SyntaxTree::parse_with_dialect(&rewritten, dialect).is_ok();
        let edit_count = edits.len();
        let preview = bounded_preview(&rewritten, request.max_preview_bytes);
        files.push(RefactorPreviewFile {
            path: file.clone(),
            dialect,
            changed,
            written: false,
            edit_count,
            edits,
            input_bytes: input.text.len(),
            output_bytes: rewritten.len(),
            output_parse_ok,
            input_hash: stable_text_hash(&input.text),
            output_hash: stable_text_hash(&rewritten),
            preview,
            rewritten,
        });
    }

    if request.mode == RefactorPreviewMode::Function && total_definitions == 0 {
        anyhow::bail!(
            "function '{}' was not found in callable definitions",
            request.from.as_str()
        );
    }

    let changed_files = files
        .iter()
        .filter(|file| file.changed)
        .map(|file| file.path.display().to_string())
        .collect::<Vec<_>>();

    let summary = RefactorPreviewSummary::new(
        changed_files,
        files.iter().filter(|file| !file.changed).count(),
        total_definitions,
        total_target_occurrences,
        files.iter().map(|file| file.edit_count).sum(),
        files.iter().filter(|file| !file.output_parse_ok).count(),
    );
    let policy = evaluate_refactor_preview_policy(request.policy_options, &summary);

    Ok(RefactorPreview {
        workspace: request.workspace,
        mode: request.mode,
        from: request.from.as_str().to_owned(),
        to: request.to.as_str().to_owned(),
        write_requested: request.write,
        files,
        summary,
        policy,
    })
}
