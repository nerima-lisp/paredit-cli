//! Duplicate test name detection across explicit files.

pub use crate::duplicate_test_name::domain::{
    DuplicateTestNameItem, build_duplicate_test_name_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DuplicateTestNameItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} shadowed test name(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
