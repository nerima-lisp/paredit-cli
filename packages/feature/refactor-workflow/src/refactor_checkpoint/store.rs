//! Where checkpoints live between invocations.
//!
//! Same convention as `.paredit/kill-ring.json`
//! ([`paredit_core_cli::kill_ring`]): a repository-relative dotfile rather
//! than a user-global one, so two agents working two checkouts do not share
//! checkpoints by accident, and an environment variable override for a
//! caller that genuinely wants one location. One checkpoint is one file
//! rather than entries in a shared ring, because checkpoints are named,
//! independently created and deleted, and a single JSON array would make
//! every write contend on the same file for no reason.

use std::path::{Path, PathBuf};

use paredit_core_cli::shared::{read_file_or_empty, write_artifact_with_rollback};
use paredit_core_cli::{CliError, CliResult};

use super::domain::{Checkpoint, checkpoint_file_name};

/// Where checkpoints live when `--checkpoints-dir` is not given.
const DEFAULT_CHECKPOINTS_DIR: &str = ".paredit/checkpoints";

/// The environment variable that overrides the default directory.
const CHECKPOINTS_DIR_VARIABLE: &str = "PAREDIT_CHECKPOINTS_DIR";

/// Resolves the checkpoint directory from the flag, the environment, then the
/// default, in that order of precedence.
#[must_use]
pub fn checkpoints_dir(explicit: Option<PathBuf>) -> PathBuf {
    explicit
        .or_else(|| std::env::var_os(CHECKPOINTS_DIR_VARIABLE).map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CHECKPOINTS_DIR))
}

#[must_use]
pub fn checkpoint_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(checkpoint_file_name(name))
}

/// Reads one checkpoint, or `None` if no checkpoint has that name.
///
/// A malformed file is an error rather than "not found": silently treating a
/// corrupted checkpoint as absent would let `--force` overwrite it, and
/// `restore`/`delete` report the wrong thing if it looked like it never
/// existed.
pub fn read_checkpoint(dir: &Path, name: &str) -> CliResult<Option<Checkpoint>> {
    let path = checkpoint_path(dir, name);
    let (input, existed) = read_file_or_empty(&path)?;
    if !existed || input.text.trim().is_empty() {
        return Ok(None);
    }

    let value: serde_json::Value =
        serde_json::from_str(&input.text).map_err(|source| CliError::Json {
            context: format!("checkpoint {} is not valid JSON", path.display()),
            source,
        })?;
    let checkpoint = Checkpoint::from_json(&value).map_err(|error| CliError::Io {
        context: format!("checkpoint {} is unusable: {error}", path.display()),
        source: std::io::Error::other(error.to_string()),
    })?;
    Ok(Some(checkpoint))
}

/// Writes a checkpoint, creating the checkpoint directory if it is missing.
pub fn write_checkpoint(dir: &Path, checkpoint: &Checkpoint) -> CliResult<()> {
    if !dir.as_os_str().is_empty() && !dir.exists() {
        std::fs::create_dir_all(dir).map_err(CliError::io(format!(
            "failed to create checkpoint directory {}",
            dir.display()
        )))?;
    }

    let mut rendered =
        serde_json::to_string_pretty(&checkpoint.to_json()).map_err(|source| CliError::Json {
            context: format!("failed to render checkpoint {}", checkpoint.name),
            source,
        })?;
    rendered.push('\n');
    write_artifact_with_rollback(checkpoint_path(dir, &checkpoint.name), rendered)
}

/// Removes a checkpoint's file, reporting whether it existed.
pub fn delete_checkpoint_file(dir: &Path, name: &str) -> CliResult<bool> {
    let path = checkpoint_path(dir, name);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).map_err(CliError::io(format!(
            "failed to delete checkpoint {}",
            path.display()
        ))),
    }
}

/// Every checkpoint's name, sorted, read off the directory listing.
///
/// A missing directory is an empty list: no checkpoint has ever been created
/// there, which is not a failure.
pub fn list_checkpoint_names(dir: &Path) -> CliResult<Vec<String>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).map_err(CliError::io(format!(
                "failed to list checkpoint directory {}",
                dir.display()
            )));
        }
    };

    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(CliError::io(format!(
            "failed to read checkpoint directory {}",
            dir.display()
        )))?;
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some("json") {
            continue;
        }
        if let Some(name) = path.file_stem().and_then(std::ffi::OsStr::to_str) {
            names.push(name.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_explicit_directory_wins_over_the_default() {
        assert_eq!(
            checkpoints_dir(Some(PathBuf::from("/tmp/checkpoints"))),
            PathBuf::from("/tmp/checkpoints")
        );
    }

    #[test]
    fn a_missing_directory_lists_no_checkpoints() {
        let dir = std::env::temp_dir().join(format!(
            "paredit-checkpoints-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        assert_eq!(
            list_checkpoint_names(&dir).expect("list"),
            Vec::<String>::new()
        );
        assert!(read_checkpoint(&dir, "anything").expect("read").is_none());
    }
}
