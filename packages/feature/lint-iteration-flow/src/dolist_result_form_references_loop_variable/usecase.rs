//! `dolist` result forms reading the loop variable, across explicit files.

pub use crate::dolist_result_form_references_loop_variable::domain::{
    DolistResultVariableItem, build_dolist_result_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, matching every other report in the
/// suite.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DolistResultVariableItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} dolist result form(s) reading the loop variable",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
