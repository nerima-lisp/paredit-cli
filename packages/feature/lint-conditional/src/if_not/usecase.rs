//! If-not (`(if test nil t)` is `(not test)`) detection.

pub use crate::if_not::domain::{IfNotItem, build_if_not_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. `(if test nil t)` is a negation
/// written the long way, but it is a build-breaking defect only in a project
/// that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<IfNotItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} (if test nil t) form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
