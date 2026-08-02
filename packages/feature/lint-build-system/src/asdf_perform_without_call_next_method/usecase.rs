//! Non-composing primary `perform` method detection across explicit files.

pub use crate::asdf_perform_without_call_next_method::domain::{
    AsdfPerformWithoutCallNextMethodItem, build_asdf_perform_without_call_next_method_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: fully replacing the standard method
/// is a legitimate, if drastic, choice, and only a project that has decided
/// otherwise should fail its build over it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<AsdfPerformWithoutCallNextMethodItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} perform method(s) that never call call-next-method",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
