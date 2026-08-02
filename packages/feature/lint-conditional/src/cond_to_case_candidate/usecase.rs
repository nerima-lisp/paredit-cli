//! Cond-to-`case` candidate detection across explicit files.

pub use crate::cond_to_case_candidate::domain::{
    CondToCaseItem, build_cond_to_case_candidate_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A `cond` that dispatches on one
/// variable is equivalent to the `case` it could be written as, so it is a
/// build-breaking defect only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<CondToCaseItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} cond form(s) that dispatch on one variable",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
