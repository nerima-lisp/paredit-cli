//! Explicit-step-delta (`(incf x 1)` is `(incf x)`) detection across explicit
//! files.

pub use crate::explicit_step_delta::domain::{
    ExplicitStepDeltaItem, build_explicit_step_delta_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A redundant delta is noise, not a
/// bug, so it breaks a build only in a project that has decided it should.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ExplicitStepDeltaItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} explicit default step delta(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
