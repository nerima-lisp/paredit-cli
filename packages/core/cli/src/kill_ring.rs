//! The one piece of state this tool keeps between invocations.
//!
//! `paredit.el` can cut and paste because Emacs has a kill ring living in the
//! session. A CLI has no session, so the ring has to be a file — and making it
//! a *named* file rather than a hidden global is the design decision the
//! feature turns on:
//!
//! - Two agents refactoring two checkouts do not share a clipboard by
//!   accident. The default path is repository-relative, not user-relative.
//! - A ring you can point at is a ring you can read, diff, commit, or delete.
//!   An implicit one is only observable through the command that reads it.
//! - `$PAREDIT_KILL_RING` overrides the default, so a caller that genuinely
//!   wants a process-wide ring can have one by saying so.
//!
//! Entries are newest-first, so `--index 0` is the most recent kill with no
//! arithmetic, and the ring is truncated to `--ring-size` on every push.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::error::{CliError, CliResult};
use crate::shared::{read_file_or_empty, write_artifact_with_rollback};

/// Where the ring lives when `--ring` is not given.
const DEFAULT_RING_PATH: &str = ".paredit/kill-ring.json";

/// The environment variable that overrides the default path.
const RING_PATH_VARIABLE: &str = "PAREDIT_KILL_RING";

/// The on-disk schema version, so a future shape change is detectable rather
/// than silently misread as an empty ring.
///
/// `pub` so a caller that wants to *diagnose* a ring file — rather than only
/// read or write one — can compare what is on disk against what this build
/// writes. `read_ring` itself never reads this field back; a ring's own
/// `schema_version` disagreeing with it is exactly the case `read_ring`
/// cannot tell apart from an empty ring today (see `inspect kill-ring`).
pub const SCHEMA_VERSION: u64 = 1;

/// One killed or copied form, with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KillRingEntry {
    pub text: String,
    /// The file the form was taken from, or `None` when it came from stdin.
    pub origin: Option<String>,
}

/// The ring as read from disk, newest entry first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KillRing {
    pub entries: Vec<KillRingEntry>,
}

impl KillRing {
    /// The entry `index` steps back from the newest.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&KillRingEntry> {
        self.entries.get(index)
    }

    /// Adds `entry` as the newest, dropping whatever falls past `capacity`.
    pub fn push(&mut self, entry: KillRingEntry, capacity: usize) {
        self.entries.insert(0, entry);
        self.entries.truncate(capacity.max(1));
    }
}

/// Resolves the ring path from the flag, the environment, then the default.
#[must_use]
pub fn ring_path(explicit: Option<PathBuf>) -> PathBuf {
    explicit
        .or_else(|| std::env::var_os(RING_PATH_VARIABLE).map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_RING_PATH))
}

/// Reads the ring, treating a missing file as an empty one.
///
/// A malformed file is an error rather than an empty ring: silently discarding
/// somebody's clipboard because a byte got corrupted is worse than refusing.
pub fn read_ring(path: &Path) -> CliResult<KillRing> {
    let (input, existed) = read_file_or_empty(path)?;
    if !existed || input.text.trim().is_empty() {
        return Ok(KillRing::default());
    }

    let document: Value = serde_json::from_str(&input.text).map_err(|error| CliError::Json {
        context: format!("kill ring {} is not valid JSON", path.display()),
        source: error,
    })?;
    let entries = document
        .get("entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    Some(KillRingEntry {
                        text: entry.get("text")?.as_str()?.to_owned(),
                        origin: entry
                            .get("origin")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(KillRing { entries })
}

/// Writes the ring back, creating the containing directory if it is missing.
pub fn write_ring(path: &Path, ring: &KillRing) -> CliResult<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(CliError::io(format!(
                "failed to create kill ring directory {}",
                parent.display()
            )))?;
        }
    }

    let document = json!({
        "schema_version": SCHEMA_VERSION,
        "entries": ring
            .entries
            .iter()
            .map(|entry| json!({ "text": entry.text, "origin": entry.origin }))
            .collect::<Vec<_>>(),
    });
    let mut rendered = serde_json::to_string_pretty(&document).map_err(|error| CliError::Json {
        context: "failed to render the kill ring".to_owned(),
        source: error,
    })?;
    rendered.push('\n');
    write_artifact_with_rollback(path.to_path_buf(), rendered)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn push_keeps_the_newest_first_and_drops_past_capacity() {
        let mut ring = KillRing::default();
        for name in ["a", "b", "c"] {
            ring.push(
                KillRingEntry {
                    text: name.to_owned(),
                    origin: None,
                },
                2,
            );
        }
        assert_eq!(ring.entries.len(), 2);
        assert_eq!(ring.get(0).unwrap().text, "c");
        assert_eq!(ring.get(1).unwrap().text, "b");
    }

    #[test]
    fn a_capacity_of_zero_still_keeps_the_entry_just_pushed() {
        let mut ring = KillRing::default();
        ring.push(
            KillRingEntry {
                text: "a".to_owned(),
                origin: None,
            },
            0,
        );
        assert_eq!(ring.entries.len(), 1);
    }

    #[test]
    fn the_explicit_path_wins_over_the_default() {
        assert_eq!(
            ring_path(Some(PathBuf::from("/tmp/ring.json"))),
            PathBuf::from("/tmp/ring.json")
        );
    }
}
