//! One-step-arithmetic (`(+ x 1)` is `(1+ x)`; `(- x 1)` is `(1- x)`) detection
//! across explicit files.

pub use crate::one_step_arithmetic::domain::{
    OneStepArithmeticItem, build_one_step_arithmetic_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A spelled-out unit step is correct
/// code, so it is a build-breaking finding only in a project that has decided
/// it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<OneStepArithmeticItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} one-step arithmetic form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
