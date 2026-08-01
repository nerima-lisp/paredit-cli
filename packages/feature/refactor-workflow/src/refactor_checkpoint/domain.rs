//! The checkpoint's own shape, independent of where it is stored.

use std::time::{SystemTime, UNIX_EPOCH};

use paredit_core_safety::journal::UndoJournal;
use serde_json::{Value, json};

/// The on-disk format version, so a future shape change is detectable rather
/// than silently misread.
pub const CHECKPOINT_SCHEMA_VERSION: u64 = 1;

/// A named snapshot of the files it covers, at the moment it was taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub name: String,
    /// Unix seconds. No date-time dependency for one integer field.
    pub created_at_unix: u64,
    pub journal: UndoJournal,
}

/// Why a checkpoint could not be built, read, or referenced.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CheckpointError {
    #[error(
        "checkpoint name {name:?} is empty, or is not made of ASCII letters, digits, `-`, \
         `_` or `.`, or starts with `.`"
    )]
    InvalidName { name: String },
    #[error("checkpoint document is not valid JSON: {0}")]
    Malformed(String),
    #[error("checkpoint has schema version {found}, expected {expected}")]
    UnsupportedSchema { found: u64, expected: u64 },
}

/// Rejects everything that is not a safe, unambiguous filename component.
///
/// The name becomes `<name>.json` under the checkpoint directory with no
/// further escaping, so this is also the path-traversal guard: no `/`, no
/// `..`, no leading `.` that would collide with a dotfile.
pub fn validate_checkpoint_name(name: &str) -> Result<(), CheckpointError> {
    let is_valid = !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if is_valid {
        Ok(())
    } else {
        Err(CheckpointError::InvalidName {
            name: name.to_owned(),
        })
    }
}

/// The file a checkpoint named `name` lives at, relative to its directory.
#[must_use]
pub fn checkpoint_file_name(name: &str) -> String {
    format!("{name}.json")
}

#[must_use]
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

impl Checkpoint {
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": CHECKPOINT_SCHEMA_VERSION,
            "name": self.name,
            "created_at_unix": self.created_at_unix,
            "journal": self.journal.to_json(),
        })
    }

    pub fn from_json(value: &Value) -> Result<Self, CheckpointError> {
        let object = value.as_object().ok_or_else(|| {
            CheckpointError::Malformed("checkpoint must be a JSON object".to_owned())
        })?;

        let schema_version = object
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                CheckpointError::Malformed(
                    "checkpoint field schema_version must be a number".into(),
                )
            })?;
        if schema_version != CHECKPOINT_SCHEMA_VERSION {
            return Err(CheckpointError::UnsupportedSchema {
                found: schema_version,
                expected: CHECKPOINT_SCHEMA_VERSION,
            });
        }

        let name = object
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                CheckpointError::Malformed("checkpoint field name must be a string".to_owned())
            })?
            .to_owned();
        let created_at_unix = object
            .get("created_at_unix")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                CheckpointError::Malformed(
                    "checkpoint field created_at_unix must be a number".to_owned(),
                )
            })?;
        let journal_value = object.get("journal").ok_or_else(|| {
            CheckpointError::Malformed("checkpoint field journal must be present".to_owned())
        })?;
        let journal = UndoJournal::from_json(journal_value)
            .map_err(|error| CheckpointError::Malformed(error.to_string()))?;

        Ok(Self {
            name,
            created_at_unix,
            journal,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_name_is_valid() {
        assert_eq!(validate_checkpoint_name("before-rename_v2.1"), Ok(()));
    }

    #[test]
    fn an_empty_name_is_rejected() {
        assert!(validate_checkpoint_name("").is_err());
    }

    #[test]
    fn a_name_starting_with_a_dot_is_rejected() {
        assert!(validate_checkpoint_name(".hidden").is_err());
    }

    #[test]
    fn a_name_containing_a_path_separator_is_rejected() {
        assert!(validate_checkpoint_name("../escape").is_err());
        assert!(validate_checkpoint_name("a/b").is_err());
    }

    #[test]
    fn a_name_containing_whitespace_is_rejected() {
        assert!(validate_checkpoint_name("has space").is_err());
    }

    #[test]
    fn an_overlong_name_is_rejected() {
        assert!(validate_checkpoint_name(&"a".repeat(201)).is_err());
    }

    #[test]
    fn a_checkpoint_round_trips_through_json() {
        use paredit_core_safety::journal::{UndoJournal, UndoJournalFile};
        use std::path::PathBuf;

        let journal = UndoJournal::new(
            "refactor create-checkpoint",
            vec![
                UndoJournalFile::record(PathBuf::from("src/core.lisp"), "(a)\n", &[])
                    .expect("record identity entry"),
            ],
        );
        let checkpoint = Checkpoint {
            name: "before-rename".to_owned(),
            created_at_unix: 1_700_000_000,
            journal,
        };

        let parsed = Checkpoint::from_json(&checkpoint.to_json()).expect("parse checkpoint");
        assert_eq!(parsed, checkpoint);
    }

    #[test]
    fn a_checkpoint_from_a_future_schema_is_refused() {
        let mut value = Checkpoint {
            name: "x".to_owned(),
            created_at_unix: 0,
            journal: UndoJournal::new("refactor create-checkpoint", Vec::new()),
        }
        .to_json();
        value["schema_version"] = serde_json::json!(99);

        assert_eq!(
            Checkpoint::from_json(&value),
            Err(CheckpointError::UnsupportedSchema {
                found: 99,
                expected: CHECKPOINT_SCHEMA_VERSION,
            })
        );
    }
}
