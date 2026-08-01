//! an :around method whose body never calls call-next-method, across explicit files.

pub use crate::around_method_missing_call_next_method::domain::{
    AroundMethodMissingCallNextMethodItem, build_around_method_missing_call_next_method_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: this is a finding a project decides
/// is build-breaking, not one this tool decides for it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<AroundMethodMissingCallNextMethodItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} :around method(s) that never call call-next-method",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
