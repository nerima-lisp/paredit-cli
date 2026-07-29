//! The file set `migrate run` walks.
//!
//! The same `WorkspaceInputArgs` surface the `query` namespace takes, and for
//! the same reason: a migration's natural unit is a repository, and
//! `--since origin/main` is what makes "did this branch introduce anything the
//! migration has not been run over" a CI question.

use std::path::PathBuf;

use paredit_core_cli::error::CliResult;
use paredit_core_cli::workspace_args::{WorkspaceInputArgs, scan_workspace};

/// Resolves `input` against `roots` into the files to read, in scan order.
pub fn selected_files(input: &WorkspaceInputArgs, roots: &[PathBuf]) -> CliResult<Vec<PathBuf>> {
    let resolved = input.resolve(roots)?;
    let cache = input.cache()?;
    let (discovery, _) = scan_workspace(&resolved.options, resolved.from_list, cache.as_ref())?;
    Ok(discovery.files().to_vec())
}
