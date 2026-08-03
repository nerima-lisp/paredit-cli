//! with-accessors-empty-binding-list detection.

pub use crate::with_accessors_empty_binding_list::domain::{
    WithAccessorsEmptyBindingListItem, build_with_accessors_empty_binding_list_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, like every other report in this
/// package: the finding is worth surfacing, but it is a build-breaking one only
/// in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<WithAccessorsEmptyBindingListItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} empty with-slots/with-accessors binding list(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
