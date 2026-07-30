//! Sign-comparison (`(= x 0)`/`(> x 0)`/`(< x 0)`, better written with
//! `zerop`/`plusp`/`minusp`) detection across explicit files.

pub use crate::sign_comparison::domain::{SignComparisonItem, build_sign_comparison_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A comparison against 0 is correct
/// code that reads worse than the predicate it is equivalent to, so it is a
/// build-breaking finding only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SignComparisonItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} sign comparison(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
