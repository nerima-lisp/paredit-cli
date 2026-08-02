//! Sleep-in-test detection across explicit files.

pub use crate::sleep_in_test::domain::{SleepInTestItem, build_sleep_in_test_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SleepInTestItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} sleeping call(s) inside tests",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
