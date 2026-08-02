//! Assertion-free test detection across explicit files.

pub use crate::test_without_assertion::domain::{
    TestWithoutAssertionItem, build_test_without_assertion_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A test that asserts nothing is worth
/// knowing about, but whether it should stop a build is a project's decision.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<TestWithoutAssertionItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} test(s) with no assertion",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
