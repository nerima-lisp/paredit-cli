//! Value-returning `handler-bind` handler detection across explicit files.

pub use crate::handler_bind_handler_returns_bare_value::domain::{
    HandlerBindHandlerReturnsBareValueItem, build_handler_bind_handler_returns_bare_value_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<HandlerBindHandlerReturnsBareValueItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} handler-bind handler(s) whose value is discarded",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
