//! Independent-binding `let*` detection across explicit files.

pub use crate::let_star_independent_bindings::domain::{
    LetStarIndependentBindingsItem, build_let_star_independent_bindings_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<LetStarIndependentBindingsItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} let* form(s) with independent bindings",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
