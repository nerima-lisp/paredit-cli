//! Duplicate `case`/`ecase`/`ccase` key detection across explicit files.

pub use crate::duplicate_case_keys::domain::{
    DuplicateCaseKeyItem, build_duplicate_case_key_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A repeated key makes a clause dead,
/// but failing the build on it is a choice a project makes rather than one this
/// tool makes for it.
#[must_use]
pub fn evaluate_fail_on_duplicate_policy(
    fail_on_duplicate: bool,
    reports: &[FileFindings<DuplicateCaseKeyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_duplicate.then_some("--fail-on-duplicate"),
        reports,
        |report| {
            format!(
                "{} has {} duplicated case key(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
