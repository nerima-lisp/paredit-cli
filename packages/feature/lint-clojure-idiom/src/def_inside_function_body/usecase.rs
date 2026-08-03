//! `def-inside-function-body` detection across explicit files.

pub use crate::def_inside_function_body::domain::{
    DefInsideFunctionBodyItem, build_def_inside_function_body_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DefInsideFunctionBodyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} def form(s) evaluated inside a function body",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
