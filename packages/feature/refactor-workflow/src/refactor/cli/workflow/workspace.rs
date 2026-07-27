use super::super::types::plan::WorkspaceRefactorPlanDiscovery;
use anyhow::Result;
use paredit_core_workspace::workspace::WorkspaceDiscoveryOptions;
use paredit_core_workspace::workspace::discover_workspace_files;
use std::path::PathBuf;

// Public since the extraction: crate-internal visibility cannot cross a
// crate boundary, so this lint applies for the first time.
#[derive(Debug)]
pub struct WorkspaceRefactorScope {
    pub paths: Vec<PathBuf>,
    pub workspace: WorkspaceRefactorPlanDiscovery,
}

pub fn discover_workspace_refactor_scope(
    options: WorkspaceDiscoveryOptions,
) -> Result<WorkspaceRefactorScope> {
    let discovery = discover_workspace_files(&options)?;
    let skipped_unknown_count = discovery.skipped_unknown_count();
    let skipped_hidden_count = discovery.skipped_hidden_count();
    let skipped_generated_count = discovery.skipped_generated_count();
    let skipped_symlink_count = discovery.skipped_symlink_count();
    let paths = discovery.into_files();
    let workspace = WorkspaceRefactorPlanDiscovery {
        roots: options.roots,
        discovered_file_count: paths.len(),
        skipped_unknown_count,
        skipped_hidden_count,
        skipped_generated_count,
        skipped_symlink_count,
    };

    Ok(WorkspaceRefactorScope { paths, workspace })
}
