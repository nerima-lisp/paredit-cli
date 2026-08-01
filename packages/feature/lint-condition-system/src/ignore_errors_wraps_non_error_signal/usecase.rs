//! Escaping-`signal`-under-`ignore-errors` detection across explicit files.

pub use crate::ignore_errors_wraps_non_error_signal::domain::{
    IgnoreErrorsWrapsNonErrorSignalItem, build_ignore_errors_wraps_non_error_signal_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<IgnoreErrorsWrapsNonErrorSignalItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} signal(s) that escape their ignore-errors wrapper",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
