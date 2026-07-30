//! Multiple-value-list-of-values ((multiple-value-list (values a b)) is (list a b)) detection.

pub use crate::multiple_value_list_of_values::domain::{
    MultipleValueListOfValuesItem, build_multiple_value_list_of_values_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Routing a literal `values` through the
/// multiple-values machinery is correct code stated indirectly, so it is
/// build-breaking only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<MultipleValueListOfValuesItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} multiple-value-list of a values form",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
