//! `future-promise-never-realized` detection across explicit files.

pub use crate::future_promise_never_realized::domain::{
    FuturePromiseNeverRealizedItem, build_future_promise_never_realized_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<FuturePromiseNeverRealizedItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} future/promise binding(s) never mentioned in the body",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
