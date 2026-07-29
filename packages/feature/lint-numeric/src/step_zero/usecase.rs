//! Step-zero ((incf x 0) is a no-op) detection.

pub use crate::step_zero::domain::{StepZeroItem, build_step_zero_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A zero step is dead code, but
/// dropping the form could discard the place's side effects, so whether it is
/// build-breaking is a project's call rather than this rule's.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<StepZeroItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} zero-step form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
