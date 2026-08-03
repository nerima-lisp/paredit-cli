//! `contains-on-non-associative` detection across explicit files.

pub use crate::contains_on_non_associative::domain::{
    ContainsOnNonAssociativeItem, build_contains_on_non_associative_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ContainsOnNonAssociativeItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} contains? call(s) that can never answer true",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
