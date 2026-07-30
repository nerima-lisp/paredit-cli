//! Redundant last count of 1 ((last x 1) is (last x)) detection.

pub use crate::last_default_count::domain::{
    LastDefaultCountItem, build_last_default_count_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A restated default is noise, but it
/// is a build-breaking defect only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<LastDefaultCountItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant last count(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
