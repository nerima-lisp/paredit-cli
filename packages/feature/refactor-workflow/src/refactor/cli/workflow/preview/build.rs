use super::super::super::super::*;
use super::super::super::args::RefactorPreviewMode;
use super::super::super::types::plan::WorkspaceRefactorPlanDiscovery;
use super::super::super::types::preview::{RefactorPreview, RefactorPreviewFile};

pub(in crate::presentation::cli::refactor::workflow) struct BuildRefactorPreviewRequest<'a> {
    pub(in crate::presentation::cli::refactor::workflow) paths: &'a [PathBuf],
    pub(in crate::presentation::cli::refactor::workflow) dialect: Option<DialectArg>,
    pub(in crate::presentation::cli::refactor::workflow) from: &'a SymbolName,
    pub(in crate::presentation::cli::refactor::workflow) to: &'a SymbolName,
    pub(in crate::presentation::cli::refactor::workflow) mode: RefactorPreviewMode,
    pub(in crate::presentation::cli::refactor::workflow) max_preview_bytes: usize,
    pub(in crate::presentation::cli::refactor::workflow) write: bool,
    pub(in crate::presentation::cli::refactor::workflow) policy_options:
        DomainRefactorPreviewPolicyOptions,
    pub(in crate::presentation::cli::refactor::workflow) workspace:
        Option<WorkspaceRefactorPlanDiscovery>,
}

pub(in crate::presentation::cli::refactor::workflow) fn build_refactor_preview(
    request: BuildRefactorPreviewRequest<'_>,
) -> Result<RefactorPreview> {
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
