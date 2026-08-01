//! `:report`-less `restart-case` clause detection across explicit files.

pub use crate::restart_case_clause_without_report::domain::{
    RestartCaseClauseWithoutReportItem, build_restart_case_clause_without_report_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. An unexplained restart is a real
/// defect in a program's debugger experience, but it is a build-breaking one
/// only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RestartCaseClauseWithoutReportItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} restart-case clause(s) with no :report",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
