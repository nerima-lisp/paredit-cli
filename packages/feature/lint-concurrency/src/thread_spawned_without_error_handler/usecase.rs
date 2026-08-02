//! `thread-spawned-without-error-handler` detection across explicit files.

pub use crate::thread_spawned_without_error_handler::domain::{
    ThreadSpawnedWithoutErrorHandlerItem, build_thread_spawned_without_error_handler_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ThreadSpawnedWithoutErrorHandlerItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} thread body/bodies with no error handler",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
