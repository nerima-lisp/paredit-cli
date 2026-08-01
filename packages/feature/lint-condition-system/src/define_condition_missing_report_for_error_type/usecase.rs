//! Report-less error condition detection across explicit files.

pub use crate::define_condition_missing_report_for_error_type::domain::{
    DefineConditionMissingReportForErrorTypeItem,
    build_define_condition_missing_report_for_error_type_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DefineConditionMissingReportForErrorTypeItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} defines {} error condition(s) with no :report",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
