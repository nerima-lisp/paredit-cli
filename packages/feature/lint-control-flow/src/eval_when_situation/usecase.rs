//! Eval-when-situation (an eval-when with an invalid situation keyword)
//! detection across explicit files.

pub use crate::eval_when_situation::domain::{
    EvalWhenSituationItem, build_eval_when_situation_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A misspelled situation silently
/// never runs its body at the intended time, but it is a build-breaking defect
/// only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<EvalWhenSituationItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} invalid eval-when situation(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
