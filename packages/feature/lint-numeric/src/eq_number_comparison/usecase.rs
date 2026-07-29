//! `eq`-on-a-number-literal detection across explicit files.

pub use crate::eq_number_comparison::domain::{
    EqNumberComparisonItem, build_eq_number_comparison_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. `eq` on a number is a defect, but it
/// is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<EqNumberComparisonItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} eq-on-a-number comparison(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
