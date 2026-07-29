//! T-comparison (`(eq X t)` and friends, a generalized-boolean smell) detection
//! across explicit files.

pub use crate::t_comparison::domain::{TComparisonItem, build_t_comparison_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Comparing against `t` is usually a
/// misunderstanding of generalized booleans, but it is occasionally a
/// deliberate symbol test, so only a project that has ruled that out wants a
/// build break.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<TComparisonItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} t comparison(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
