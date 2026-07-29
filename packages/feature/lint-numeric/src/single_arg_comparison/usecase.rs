//! Single-argument numeric comparison (`(< x)`, `(= x)`, … — vacuously true)
//! detection across explicit files.

pub use crate::single_arg_comparison::domain::{
    SingleArgComparisonItem, build_single_arg_comparison_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A vacuously true comparison is very
/// likely a missing operand, but the call is legal and no compiler rejects it,
/// so breaking the build on one is a project's decision.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SingleArgComparisonItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} single-argument comparison(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
