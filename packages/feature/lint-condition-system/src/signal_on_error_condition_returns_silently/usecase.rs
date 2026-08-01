//! `signal`-of-an-error detection across explicit files.

pub use crate::signal_on_error_condition_returns_silently::domain::{
    SignalOnErrorConditionReturnsSilentlyItem,
    build_signal_on_error_condition_returns_silently_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SignalOnErrorConditionReturnsSilentlyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} signals {} error condition(s) with signal rather than error",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
