//! The file set the three `query` commands share.
//!
//! All three take the full `WorkspaceInputArgs` surface — `--since`,
//! `--from-git`, `--paths-from`, the glob filters — rather than the plain
//! recursive expansion most `inspect` reports use. That is the point of
//! promoting `--query` out of the selector position: a pattern that can only
//! be asked about one named file is a debugging aid, and a pattern that can be
//! asked about "every file this branch touched" is a CI gate.

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
