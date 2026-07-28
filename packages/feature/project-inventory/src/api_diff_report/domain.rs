//! What changed between two `api-surface` snapshots, and what that means for a
//! version number.
//!
//! The SemVer question — major, minor, or patch — has one correct answer for
//! any given pair of API snapshots, and answering it by hand at release time is
//! where the mistake gets made. This answers it mechanically.
//!
//! The rules are the ones SemVer actually states, applied to a Lisp API:
//!
//! - **Breaking (major).** An export removed, an export whose required arity
//!   *rose* (calls that used to work now fail), an export whose maximum arity
//!   *fell*, or an export whose defining category changed — a function that
//!   became a macro cannot be `funcall`ed or passed as `#'`.
//! - **Compatible (minor).** An export added, or one whose accepted arity range
//!   widened. Existing calls still work.
//! - **Unchanged (patch).** Same name, same shape.
//!
//! The baseline is a snapshot rather than a git ref, deliberately: it makes the
//! comparison reproducible, lets a release check compare against a *published*
//! surface rather than whatever a working tree happens to contain, and means
//! this report never shells out.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, SyntaxTree};
use serde_json::{Value, json};

use crate::api_surface_report::domain::{ApiEntry, build_api_surface_report};

/// What a single API change requires of a version number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Impact {
    /// Same name, same shape.
    Unchanged,
    /// Existing callers keep working.
    Compatible,
    /// Existing callers break.
    Breaking,
}

impl Impact {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::Compatible => "compatible",
            Self::Breaking => "breaking",
        }
    }

    /// The smallest version bump this impact permits.
    #[must_use]
    pub const fn bump(self) -> &'static str {
        match self {
            Self::Unchanged => "patch",
            Self::Compatible => "minor",
            Self::Breaking => "major",
        }
    }
}

/// One difference between the two snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiChange {
    pub impact: Impact,
    /// `added`, `removed`, or `changed`.
    pub change: &'static str,
    pub name: String,
    pub package: String,
    pub before: Option<String>,
    pub after: Option<String>,
    /// Why this impact, in one clause.
    pub reason: &'static str,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for ApiChange {
    fn kind(&self) -> &'static str {
        self.change
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.impact.label().to_owned(),
            self.package.clone(),
            self.name.clone(),
            format!(
                "{} -> {}",
                self.before.as_deref().unwrap_or("-"),
                self.after.as_deref().unwrap_or("-")
            ),
            self.reason.to_owned(),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("impact", json!(self.impact.label())),
            ("change", json!(self.change)),
            ("name", json!(self.name)),
            ("package", json!(self.package)),
            ("before", json!(self.before)),
            ("after", json!(self.after)),
            ("reason", json!(self.reason)),
        ]
    }
}

/// One entry of a baseline snapshot, as `api-surface` wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineEntry {
    pub name: String,
    pub package: String,
    pub category: Option<String>,
    pub required_arity: Option<usize>,
    pub max_arity: Option<usize>,
}

impl BaselineEntry {
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}:{}", self.package, self.name)
    }

    #[must_use]
    pub fn signature(&self) -> String {
        format!(
            "{}/{}..{}",
            self.category.as_deref().unwrap_or("?"),
            self.required_arity
                .map_or_else(|| "?".to_owned(), |arity| arity.to_string()),
            self.max_arity
                .map_or_else(|| "*".to_owned(), |arity| arity.to_string()),
        )
    }
}

/// Reads the `files[].findings[]` of an `api-surface --output json` document.
///
/// Tolerant of extra fields and of a schema that gains them: only the five
/// keys the comparison reads are required, so a newer snapshot still diffs
/// against an older tool.
#[must_use]
pub fn read_baseline(document: &Value) -> Vec<BaselineEntry> {
    document
        .get("files")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(|file| file.get("findings")?.as_array())
                .flatten()
                .filter_map(|finding| {
                    Some(BaselineEntry {
                        name: finding.get("name")?.as_str()?.to_owned(),
                        package: finding.get("package")?.as_str()?.to_owned(),
                        category: finding
                            .get("category")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        required_arity: finding
                            .get("required_arity")
                            .and_then(Value::as_u64)
                            .map(|arity| arity as usize),
                        max_arity: finding
                            .get("max_arity")
                            .and_then(Value::as_u64)
                            .map(|arity| arity as usize),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[must_use]
pub fn build_api_diff_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    baseline: &[BaselineEntry],
) -> FileFindings<ApiChange> {
    let current = build_api_surface_report(path, dialect, tree);
    if !current.dialect_modelled {
        return FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("required_bump", json!("patch"))],
        );
    }

    let before: BTreeMap<String, &BaselineEntry> =
        baseline.iter().map(|entry| (entry.key(), entry)).collect();
    let after: BTreeMap<String, &ApiEntry> = current
        .findings
        .iter()
        .map(|entry| (entry.key(), entry))
        .collect();

    let mut changes = Vec::new();

    for (key, entry) in &after {
        match before.get(key) {
            None => changes.push(ApiChange {
                impact: Impact::Compatible,
                change: "added",
                name: entry.name.clone(),
                package: entry.package.clone(),
                before: None,
                after: Some(entry.signature()),
                reason: "a new export breaks no existing caller",
                span: entry.span,
                line: entry.line,
            }),
            Some(old) => {
                let (impact, reason) = compare(old, entry);
                if impact == Impact::Unchanged {
                    continue;
                }
                changes.push(ApiChange {
                    impact,
                    change: "changed",
                    name: entry.name.clone(),
                    package: entry.package.clone(),
                    before: Some(old.signature()),
                    after: Some(entry.signature()),
                    reason,
                    span: entry.span,
                    line: entry.line,
                });
            }
        }
    }

    for (key, entry) in &before {
        if after.contains_key(key) {
            continue;
        }
        changes.push(ApiChange {
            impact: Impact::Breaking,
            change: "removed",
            name: entry.name.clone(),
            package: entry.package.clone(),
            before: Some(entry.signature()),
            after: None,
            reason: "a removed export breaks every caller of it",
            // A removal has no site in the current source, so it anchors at the
            // start of the file rather than inventing a span.
            span: ByteSpan::new(ByteOffset::new(0), ByteOffset::new(0)),
            line: 1,
        });
    }

    let required = changes
        .iter()
        .map(|change| change.impact)
        .max()
        .unwrap_or(Impact::Unchanged);

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        changes,
        vec![
            ("baseline_export_count", json!(baseline.len())),
            ("current_export_count", json!(current.findings.len())),
            ("required_bump", json!(required.bump())),
        ],
    )
}

/// What one export's change requires.
///
/// Ordered so the first *breaking* reason wins: an export that both narrowed
/// and changed category is breaking for the reason a reader will fix first.
fn compare(before: &BaselineEntry, after: &ApiEntry) -> (Impact, &'static str) {
    if before.category != after.category && before.category.is_some() && after.category.is_some() {
        return (
            Impact::Breaking,
            "the defining form changed, so a caller's call convention may not hold",
        );
    }
    // A higher floor rejects calls that used to be accepted.
    if let (Some(old), Some(new)) = (before.required_arity, after.required_arity) {
        if new > old {
            return (Impact::Breaking, "the minimum argument count rose");
        }
    }
    // A lower ceiling does the same at the other end. `None` is unbounded, so
    // losing it is a narrowing and gaining it is a widening.
    match (before.max_arity, after.max_arity) {
        (None, Some(_)) => {
            return (Impact::Breaking, "the argument list is no longer unbounded");
        }
        (Some(old), Some(new)) if new < old => {
            return (Impact::Breaking, "the maximum argument count fell");
        }
        (Some(_), None) => {
            return (Impact::Compatible, "the argument list became unbounded");
        }
        _ => {}
    }
    if before.required_arity > after.required_arity || before.max_arity < after.max_arity {
        return (Impact::Compatible, "the accepted argument range widened");
    }
    if before.signature() == after.signature() {
        return (Impact::Unchanged, "unchanged");
    }
    (Impact::Compatible, "the signature changed compatibly")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline(entries: &[(&str, &str, Option<usize>, Option<usize>)]) -> Vec<BaselineEntry> {
        entries
            .iter()
            .map(|(package, name, required, max)| BaselineEntry {
                name: (*name).to_owned(),
                package: (*package).to_owned(),
                category: Some("defun".to_owned()),
                required_arity: *required,
                max_arity: *max,
            })
            .collect()
    }

    fn report(source: &str, baseline: &[BaselineEntry]) -> FileFindings<ApiChange> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_api_diff_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree, baseline)
    }

    fn bump(report: &FileFindings<ApiChange>) -> String {
        report
            .summary
            .iter()
            .find(|(name, _)| *name == "required_bump")
            .and_then(|(_, value)| value.as_str())
            .expect("a bump is reported")
            .to_owned()
    }

    const ONE_ARG: &str = "(defpackage :app (:export #:f))\n(defun f (a) a)";

    #[test]
    fn an_identical_surface_requires_only_a_patch() {
        let report = report(ONE_ARG, &baseline(&[("APP", "F", Some(1), Some(1))]));
        assert_eq!(bump(&report), "patch");
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_new_export_is_compatible() {
        let report = report(ONE_ARG, &[]);
        assert_eq!(report.findings[0].impact, Impact::Compatible);
        assert_eq!(report.findings[0].kind(), "added");
        assert_eq!(bump(&report), "minor");
    }

    #[test]
    fn a_removed_export_is_breaking() {
        let report = report(
            "(defpackage :app (:export #:f))\n(defun f (a) a)",
            &baseline(&[
                ("APP", "F", Some(1), Some(1)),
                ("APP", "GONE", Some(0), Some(0)),
            ]),
        );
        let removed = report
            .findings
            .iter()
            .find(|change| change.kind() == "removed")
            .expect("the removal is reported");
        assert_eq!(removed.impact, Impact::Breaking);
        assert_eq!(bump(&report), "major");
    }

    #[test]
    fn a_higher_minimum_arity_is_breaking() {
        let report = report(
            "(defpackage :app (:export #:f))\n(defun f (a b) (list a b))",
            &baseline(&[("APP", "F", Some(1), Some(1))]),
        );
        assert_eq!(report.findings[0].impact, Impact::Breaking);
        assert_eq!(report.findings[0].reason, "the minimum argument count rose");
    }

    #[test]
    fn a_lower_maximum_arity_is_breaking() {
        let report = report(
            "(defpackage :app (:export #:f))\n(defun f (a) a)",
            &baseline(&[("APP", "F", Some(1), Some(3))]),
        );
        assert_eq!(report.findings[0].impact, Impact::Breaking);
    }

    #[test]
    fn losing_an_unbounded_argument_list_is_breaking() {
        let report = report(
            "(defpackage :app (:export #:f))\n(defun f (a) a)",
            &baseline(&[("APP", "F", Some(1), None)]),
        );
        assert_eq!(report.findings[0].impact, Impact::Breaking);
        assert_eq!(
            report.findings[0].reason,
            "the argument list is no longer unbounded"
        );
    }

    #[test]
    fn a_widened_range_is_compatible() {
        let report = report(
            "(defpackage :app (:export #:f))\n(defun f (a &optional b) (list a b))",
            &baseline(&[("APP", "F", Some(1), Some(1))]),
        );
        assert_eq!(report.findings[0].impact, Impact::Compatible);
        assert_eq!(bump(&report), "minor");
    }

    #[test]
    fn a_function_that_became_a_macro_is_breaking() {
        let report = report(
            "(defpackage :app (:export #:f))\n(defmacro f (a) a)",
            &baseline(&[("APP", "F", Some(1), Some(1))]),
        );
        assert_eq!(report.findings[0].impact, Impact::Breaking);
    }

    #[test]
    fn the_worst_change_decides_the_bump() {
        let report = report(
            "(defpackage :app (:export #:f) (:export #:g))\n(defun f (a b) (list a b))\n(defun g () 1)",
            &baseline(&[("APP", "F", Some(1), Some(1))]),
        );
        // `g` was added (minor) and `f` narrowed (major); major wins.
        assert_eq!(bump(&report), "major");
    }

    #[test]
    fn a_baseline_document_is_read_from_the_surface_reports_own_json() {
        let document = json!({
            "files": [{
                "findings": [{
                    "name": "F",
                    "package": "APP",
                    "category": "defun",
                    "required_arity": 1,
                    "max_arity": 1,
                }]
            }]
        });
        let entries = read_baseline(&document);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key(), "APP:F");
        assert_eq!(entries[0].signature(), "defun/1..1");
    }

    #[test]
    fn a_baseline_document_with_no_files_reads_as_empty() {
        assert!(read_baseline(&json!({})).is_empty());
    }
}
