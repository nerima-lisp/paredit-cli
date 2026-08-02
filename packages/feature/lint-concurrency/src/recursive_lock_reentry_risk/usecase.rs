//! `recursive-lock-reentry-risk` detection across explicit files.

pub use crate::recursive_lock_reentry_risk::domain::{
    RecursiveLockReentryRiskItem, build_recursive_lock_reentry_risk_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RecursiveLockReentryRiskItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} non-recursive lock(s) retaken inside their own scope",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
