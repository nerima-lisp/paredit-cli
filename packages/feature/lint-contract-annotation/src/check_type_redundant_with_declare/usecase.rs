//! Redundant `check-type`/`declare` pair detection across explicit files.

pub use crate::check_type_redundant_with_declare::domain::{
    CheckTypeRedundantWithDeclareItem, build_check_type_redundant_with_declare_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<CheckTypeRedundantWithDeclareItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} check-type(s) restating a declare",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
