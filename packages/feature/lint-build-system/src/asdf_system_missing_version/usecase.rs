//! Missing-`:version` detection across explicit files.

pub use crate::asdf_system_missing_version::domain::{
    AsdfSystemMissingVersionItem, build_asdf_system_missing_version_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. An unversioned system is a metadata
/// gap, but a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<AsdfSystemMissingVersionItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} system(s) with no :version",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
