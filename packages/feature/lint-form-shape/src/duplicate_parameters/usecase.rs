//! Duplicate lambda-list parameter detection across explicit files.

pub use crate::duplicate_parameters::domain::{
    DuplicateParameterItem, build_duplicate_parameter_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A lambda list that names a parameter
/// twice is a program error, but whether that stops a build is the project's
/// call.
#[must_use]
pub fn evaluate_fail_on_duplicate_policy(
    fail_on_duplicate: bool,
    reports: &[FileFindings<DuplicateParameterItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_duplicate.then_some("--fail-on-duplicate"),
        reports,
        |report| {
            format!(
                "{} has {} duplicated parameter(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
