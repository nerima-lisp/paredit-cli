//! Ignored-situation `eval-when` detection across explicit files.

pub use crate::eval_when_body_never_runs::domain::{
    EvalWhenBodyNeverRunsItem, build_eval_when_body_never_runs_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<EvalWhenBodyNeverRunsItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has an eval-when whose body never runs",
                report.path.display()
            )
        },
    )
}
