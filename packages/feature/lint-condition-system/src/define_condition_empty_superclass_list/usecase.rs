//! Supertype-less `define-condition` detection across explicit files.

pub use crate::define_condition_empty_superclass_list::domain::{
    DefineConditionEmptySuperclassListItem, build_define_condition_empty_superclass_list_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DefineConditionEmptySuperclassListItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} define-condition form(s) with no supertype",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
