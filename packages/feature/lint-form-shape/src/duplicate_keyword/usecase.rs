//! Duplicate-keyword ((make-instance 'c :x 1 :x 2) passes :x twice) detection.

pub use crate::duplicate_keyword::domain::{DuplicateKeywordItem, build_duplicate_keyword_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A repeated keyword silently discards
/// a value, but whether that stops a build is the project's call.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DuplicateKeywordItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} duplicate keyword argument(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
