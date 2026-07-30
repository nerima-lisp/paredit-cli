//! Negated-step-delta (`(incf x -1)` is `(decf x 1)`) detection across explicit
//! files.

pub use crate::negated_step_delta::domain::{
    NegatedStepDeltaItem, build_negated_step_delta_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A negative delta is correct code
/// stated backwards, so it is a build-breaking finding only in a project that
/// has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NegatedStepDeltaItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} negative step delta(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
