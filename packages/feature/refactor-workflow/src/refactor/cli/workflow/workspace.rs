use super::super::types::plan::WorkspaceRefactorPlanDiscovery;
use paredit_core_cli::CliResult;
use paredit_core_cli::workspace_args::{ResolvedWorkspaceInput, WorkspaceInputArgs};
use std::path::PathBuf;

// Public since the extraction: crate-internal visibility cannot cross a
// crate boundary, so this lint applies for the first time.
#[derive(Debug)]
pub struct WorkspaceRefactorScope {
    pub paths: Vec<PathBuf>,
    pub workspace: WorkspaceRefactorPlanDiscovery,
}

/// Resolves the file set a workspace refactor operates on.
///
/// `roots` is what the user typed and is reported back unchanged, while
/// `resolved.options.roots` is what the selector produced — for `--since` a
/// list of changed files rather than the directory. Reporting the latter as the
/// roots would make a plan's manifest describe the diff instead of the project.
pub fn discover_workspace_refactor_scope(
    input: &WorkspaceInputArgs,
    roots: Vec<PathBuf>,
    resolved: &ResolvedWorkspaceInput,
) -> CliResult<WorkspaceRefactorScope> {
    let (discovery, _) = input.scan(resolved)?;
    let skipped_unknown_count = discovery.skipped_unknown_count();
    let skipped_hidden_count = discovery.skipped_hidden_count();
    let skipped_generated_count = discovery.skipped_generated_count();
    let skipped_symlink_count = discovery.skipped_symlink_count();
    let paths = discovery.into_files();
    let workspace = WorkspaceRefactorPlanDiscovery {
        roots,
        discovered_file_count: paths.len(),
        skipped_unknown_count,
        skipped_hidden_count,
        skipped_generated_count,
        skipped_symlink_count,
    };

    Ok(WorkspaceRefactorScope { paths, workspace })
}
