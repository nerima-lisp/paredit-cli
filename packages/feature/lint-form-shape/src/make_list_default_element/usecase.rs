//! Redundant :initial-element nil ((make-list n :initial-element nil) is (make-list n)) detection.

pub use crate::make_list_default_element::domain::{
    MakeListDefaultElementItem, build_make_list_default_element_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A restated default is noise, but it
/// is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<MakeListDefaultElementItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant :initial-element nil",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
