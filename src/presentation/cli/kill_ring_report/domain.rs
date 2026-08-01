//! Diagnosing a kill ring file without touching it.
//!
//! `paredit_core_cli::kill_ring::read_ring` already tolerates a missing file
//! (an empty ring) and a per-entry shape mismatch (that entry is dropped).
//! What it cannot tell a caller is *which* of those happened, or whether the
//! file's own `schema_version` still matches what this build writes — both
//! questions `read_ring` silently folds into "here is whatever I could
//! read". `classify` answers them instead, by reading the same bytes through
//! `serde_json::Value` rather than through `read_ring`'s tolerant parse.

use paredit_core_cli::kill_ring::SCHEMA_VERSION;
use serde_json::Value;

/// What `classify` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::presentation::cli) enum RingStatus {
    /// No file (an empty ring by `read_ring`'s own convention, not
    /// corruption) or a well-formed one.
    Valid {
        entry_count: usize,
    },
    Corrupted(Corruption),
}

/// The specific way a kill ring file failed to read back as one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::presentation::cli) enum Corruption {
    /// The file exists and is not valid JSON at all.
    InvalidJson(String),
    /// Valid JSON, but not shaped like a kill ring: not an object, no
    /// `entries` array, or no `schema_version` field.
    MissingShape,
    /// Has an `entries` array, but `schema_version` does not match what this
    /// build writes.
    SchemaVersionMismatch { found: Value },
}

impl Corruption {
    /// A short machine-stable label for the JSON report.
    pub(in crate::presentation::cli) const fn code(&self) -> &'static str {
        match self {
            Self::InvalidJson(_) => "invalid-json",
            Self::MissingShape => "missing-shape",
            Self::SchemaVersionMismatch { .. } => "schema-version-mismatch",
        }
    }

    /// One line of prose naming what is wrong, for both the text output and
    /// the refusal message.
    pub(in crate::presentation::cli) fn message(&self) -> String {
        match self {
            Self::InvalidJson(detail) => format!("not valid JSON: {detail}"),
            Self::MissingShape => {
                "valid JSON, but missing the `entries` array or `schema_version` field \
                 a kill ring needs"
                    .to_owned()
            }
            Self::SchemaVersionMismatch { found } => {
                format!("schema_version is {found}, but this build writes {SCHEMA_VERSION}")
            }
        }
    }
}

/// Classifies `raw` as a kill ring, without writing anything.
///
/// `existed` distinguishes "no file" from "an empty file": both read as
/// `raw == ""`, and only the first is `read_ring`'s documented convention for
/// an empty ring. An empty *file* that exists is treated the same way rather
/// than as invalid JSON — trailing whitespace or a truncated write leaving
/// nothing behind is closer to "nothing here yet" than to "corrupted".
#[must_use]
pub(in crate::presentation::cli) fn classify(existed: bool, raw: &str) -> RingStatus {
    if !existed || raw.trim().is_empty() {
        return RingStatus::Valid { entry_count: 0 };
    }

    let parsed: Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(error) => return RingStatus::Corrupted(Corruption::InvalidJson(error.to_string())),
    };

    let Some(object) = parsed.as_object() else {
        return RingStatus::Corrupted(Corruption::MissingShape);
    };
    let Some(entries) = object.get("entries").and_then(Value::as_array) else {
        return RingStatus::Corrupted(Corruption::MissingShape);
    };

    match object.get("schema_version") {
        None => RingStatus::Corrupted(Corruption::MissingShape),
        Some(version) if version.as_u64() == Some(SCHEMA_VERSION) => {
            // The same "does this entry have a `text` string" test
            // `read_ring` applies, so the count reported here is the count a
            // follow-up `edit yank` would actually see.
            let entry_count = entries
                .iter()
                .filter(|entry| entry.get("text").and_then(Value::as_str).is_some())
                .count();
            RingStatus::Valid { entry_count }
        }
        Some(version) => RingStatus::Corrupted(Corruption::SchemaVersionMismatch {
            found: version.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_valid_and_empty() {
        assert_eq!(classify(false, ""), RingStatus::Valid { entry_count: 0 });
    }

    #[test]
    fn an_existing_empty_file_is_valid_and_empty() {
        assert_eq!(classify(true, ""), RingStatus::Valid { entry_count: 0 });
        assert_eq!(
            classify(true, "   \n"),
            RingStatus::Valid { entry_count: 0 }
        );
    }

    #[test]
    fn a_well_formed_ring_is_valid_with_its_entry_count() {
        let raw = format!(
            r#"{{"schema_version": {SCHEMA_VERSION}, "entries": [{{"text": "(a)", "origin": null}}, {{"text": "(b)", "origin": "f.lisp"}}]}}"#
        );
        assert_eq!(classify(true, &raw), RingStatus::Valid { entry_count: 2 });
    }

    #[test]
    fn invalid_json_is_reported_as_invalid_json() {
        let status = classify(true, "{not json");
        assert!(matches!(
            status,
            RingStatus::Corrupted(Corruption::InvalidJson(_))
        ));
    }

    #[test]
    fn json_with_no_entries_array_is_reported_as_missing_shape() {
        let status = classify(true, r#"{"schema_version": 1}"#);
        assert_eq!(status, RingStatus::Corrupted(Corruption::MissingShape));
    }

    #[test]
    fn a_json_array_at_the_top_level_is_reported_as_missing_shape() {
        let status = classify(true, "[1, 2, 3]");
        assert_eq!(status, RingStatus::Corrupted(Corruption::MissingShape));
    }

    #[test]
    fn json_with_no_schema_version_is_reported_as_missing_shape() {
        let status = classify(true, r#"{"entries": []}"#);
        assert_eq!(status, RingStatus::Corrupted(Corruption::MissingShape));
    }

    #[test]
    fn a_mismatched_schema_version_is_reported_by_name() {
        let status = classify(true, r#"{"schema_version": 999, "entries": []}"#);
        assert_eq!(
            status,
            RingStatus::Corrupted(Corruption::SchemaVersionMismatch {
                found: Value::from(999)
            })
        );
    }

    #[test]
    fn a_malformed_entry_is_silently_dropped_from_the_count_like_read_ring() {
        let raw = format!(
            r#"{{"schema_version": {SCHEMA_VERSION}, "entries": [{{"text": "(a)"}}, {{"origin": "no-text.lisp"}}]}}"#
        );
        assert_eq!(classify(true, &raw), RingStatus::Valid { entry_count: 1 });
    }
}
