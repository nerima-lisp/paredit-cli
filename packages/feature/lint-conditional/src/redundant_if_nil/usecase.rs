//! Redundant-`if`-`nil`-else (`(if test then nil)` — the explicit nil else is
//! redundant) detection across explicit files.

pub use crate::redundant_if_nil::domain::{RedundantIfNilItem, build_redundant_if_nil_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A redundant `nil` else is clutter,
/// but it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RedundantIfNilItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant nil else branch(es)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
