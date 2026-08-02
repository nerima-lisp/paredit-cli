//! Disabled-test detection across explicit files.

pub use crate::disabled_test_left_in::domain::{
    DisabledTestLeftInItem, build_disabled_test_left_in_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DisabledTestLeftInItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} disabled test(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
