//! Exhaustive-case-otherwise (a forbidden t/otherwise clause in
//! ecase/ccase/etypecase/ctypecase) detection across explicit files.

pub use crate::exhaustive_case_otherwise::domain::{
    ExhaustiveCaseOtherwiseItem, build_exhaustive_case_otherwise_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A default clause in an exhaustive
/// case form defeats the exhaustiveness check, but it is a build-breaking
/// defect only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ExhaustiveCaseOtherwiseItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} forbidden t/otherwise clause(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
