//! `lock-acquired-not-released` detection across explicit files.

pub use crate::lock_acquired_not_released::domain::{
    LockAcquiredNotReleasedItem, build_lock_acquired_not_released_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<LockAcquiredNotReleasedItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} lock acquisition(s) unprotected against a non-local exit",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
