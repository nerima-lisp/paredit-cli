//! Nil-comparison (`(eq X nil)` and friends, which are just `(null X)`)
//! detection across explicit files.

pub use crate::nil_comparison::domain::{NilComparisonItem, build_nil_comparison_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A comparison against nil is correct
/// code stated indirectly, so it is a build-breaking finding only in a project
/// that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NilComparisonItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} comparison(s) against nil",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
