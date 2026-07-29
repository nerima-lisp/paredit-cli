//! Lint baseline: a recorded set of findings to treat as "known" so
//! `inspect lint --baseline <file>` fails only on *new* findings. This is the
//! standard way to adopt a linter on an existing codebase — snapshot today's
//! findings with `--write-baseline`, commit the file, and let CI gate only on
//! regressions.
//!
//! Each finding is identified by `(path, rule, content_hash)` where
//! `content_hash` is a hash of the finding's *trimmed source line* — not its
//! line number — so a baselined finding keeps matching when unrelated lines are
//! inserted or removed above it. Exact-duplicate lines (same rule, same trimmed
//! text) collapse to one entry, which at worst over-suppresses a rare duplicate.

use std::collections::BTreeSet;

use paredit_core_cli::{CliError, CliResult};
use serde_json::{Value, json};

/// One recorded finding identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct BaselineEntry {
    pub path: String,
    pub rule: String,
    pub hash: String,
}

/// A set of known findings, loaded from or written to a baseline file.
#[derive(Debug, Default)]
pub(super) struct LintBaseline {
    entries: BTreeSet<BaselineEntry>,
}

impl LintBaseline {
    /// Builds a baseline from an iterator of finding identities.
    pub(super) fn from_entries(entries: impl IntoIterator<Item = BaselineEntry>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }

    /// Parses a baseline file's JSON contents.
    pub(super) fn parse(text: &str) -> CliResult<Self> {
        let value: Value = serde_json::from_str(text).map_err(|source| CliError::Json {
            context: "baseline file is not valid JSON".to_owned(),
            source,
        })?;
        let array = value
            .get("entries")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                paredit_core_cli::error::FeatureRefusal::message(
                    paredit_core_cli::diagnosis::ErrorCode::InputUnparsable,
                    "baseline file has no \"entries\" array",
                )
            })?;
        let mut entries = BTreeSet::new();
        for entry in array {
            let field = |name: &str| -> CliResult<String> {
                entry
                    .get(name)
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        paredit_core_cli::error::FeatureRefusal::message(
                            paredit_core_cli::diagnosis::ErrorCode::InputUnparsable,
                            format!("baseline entry is missing string field {name:?}"),
                        )
                        .into()
                    })
            };
            entries.insert(BaselineEntry {
                path: field("path")?,
                rule: field("rule")?,
                hash: field("hash")?,
            });
        }
        Ok(Self { entries })
    }

    /// Serializes the baseline to pretty JSON with entries in a stable order.
    pub(super) fn to_json(&self) -> CliResult<String> {
        let entries = self
            .entries
            .iter()
            .map(|entry| json!({ "path": entry.path, "rule": entry.rule, "hash": entry.hash }))
            .collect::<Vec<_>>();
        Ok(serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "entry_count": self.entries.len(),
            "entries": entries,
        }))?)
    }

    /// Whether a finding identity is recorded in this baseline.
    pub(super) fn contains(&self, path: &str, rule: &str, hash: &str) -> bool {
        self.entries.contains(&BaselineEntry {
            path: path.to_owned(),
            rule: rule.to_owned(),
            hash: hash.to_owned(),
        })
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, rule: &str, hash: &str) -> BaselineEntry {
        BaselineEntry {
            path: path.to_owned(),
            rule: rule.to_owned(),
            hash: hash.to_owned(),
        }
    }

    #[test]
    fn round_trips_through_json() {
        let baseline = LintBaseline::from_entries([
            entry("a.lisp", "single-arg-comparison", "fnv1a64:0001"),
            entry("b.lisp", "redundant-quote", "fnv1a64:0002"),
        ]);
        let json = baseline.to_json().expect("serialize");
        let reloaded = LintBaseline::parse(&json).expect("parse");
        assert_eq!(reloaded.len(), 2);
        assert!(reloaded.contains("a.lisp", "single-arg-comparison", "fnv1a64:0001"));
        assert!(reloaded.contains("b.lisp", "redundant-quote", "fnv1a64:0002"));
    }

    #[test]
    fn duplicate_entries_collapse() {
        let baseline = LintBaseline::from_entries([
            entry("a.lisp", "redundant-quote", "h"),
            entry("a.lisp", "redundant-quote", "h"),
        ]);
        assert_eq!(baseline.len(), 1);
    }

    #[test]
    fn does_not_contain_an_unknown_finding() {
        let baseline = LintBaseline::from_entries([entry("a.lisp", "redundant-quote", "h")]);
        assert!(!baseline.contains("a.lisp", "redundant-quote", "other"));
        assert!(!baseline.contains("a.lisp", "single-arg-comparison", "h"));
        assert!(!baseline.contains("b.lisp", "redundant-quote", "h"));
    }

    #[test]
    fn parse_rejects_malformed_json() {
        assert!(LintBaseline::parse("not json").is_err());
        assert!(LintBaseline::parse("{}").is_err());
    }

    #[test]
    fn parse_accepts_an_empty_baseline() {
        let baseline = LintBaseline::parse("{\"entries\": []}").expect("parse");
        assert_eq!(baseline.len(), 0);
    }
}
