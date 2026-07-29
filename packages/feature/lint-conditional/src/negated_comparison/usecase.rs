//! Negated-comparison (`(not (= a b))`, better written `(/= a b)`) detection
//! across explicit files.

pub use crate::negated_comparison::domain::{
    NegatedComparisonItem, build_negated_comparison_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A negated comparison is correct code
/// spelled indirectly, so it is a build-breaking defect only in a project that
/// has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NegatedComparisonItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} negated comparison(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
