//! Tautological test assertion detection across explicit files.

pub use crate::test_asserts_constant::domain::{
    ConstantShape, TestAssertsConstantItem, build_test_asserts_constant_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<TestAssertsConstantItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} assertion(s) that can never fail",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
