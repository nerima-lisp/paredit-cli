//! Duplicate-setf-place (`(setf a 1 a 2)` — a variable assigned twice in one
//! form) detection across explicit files.

pub use crate::duplicate_setf_places::domain::{
    DuplicateSetfPlaceItem, build_duplicate_setf_place_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A variable assigned twice in one form
/// is a defect, but it is a build-breaking one only in a project that has
/// decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DuplicateSetfPlaceItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} duplicate setf place(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
