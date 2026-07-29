//! Unwind-protect-no-cleanup ((unwind-protect x) is x) detection.

pub use crate::unwind_protect_no_cleanup::domain::{
    UnwindProtectNoCleanupItem, build_unwind_protect_no_cleanup_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A cleanupless `unwind-protect` is a
/// leftover wrapper, but it is a build-breaking one only in a project that has
/// decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<UnwindProtectNoCleanupItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} cleanupless unwind-protect form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
