//! Empty test body detection across explicit files.

pub use crate::empty_test_body::domain::{EmptyTestBodyItem, build_empty_test_body_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<EmptyTestBodyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} test(s) with an empty body",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
