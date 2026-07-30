//! Values-list-of-list ((values-list (list a b)) is (values a b)) detection.

pub use crate::values_list_of_list::domain::{
    ValuesListOfListItem, build_values_list_of_list_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A `(values-list (list …))` is correct
/// code that allocates a list it immediately discards, so failing a build over
/// it is a house-style decision.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ValuesListOfListItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} values-list of a fresh list",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
