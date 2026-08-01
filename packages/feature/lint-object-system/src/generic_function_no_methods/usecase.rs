//! a defgeneric no defmethod in the file ever specializes, across explicit files.

pub use crate::generic_function_no_methods::domain::{
    GenericFunctionNoMethodsItem, build_generic_function_no_methods_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: this is a finding a project decides
/// is build-breaking, not one this tool decides for it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<GenericFunctionNoMethodsItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} generic function(s) with no method",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
