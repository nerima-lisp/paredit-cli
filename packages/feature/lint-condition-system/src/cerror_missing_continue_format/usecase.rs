//! Uninformative-`cerror` detection across explicit files.

pub use crate::cerror_missing_continue_format::domain::{
    CerrorMissingContinueFormatItem, MissingContinueFormat,
    build_cerror_missing_continue_format_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<CerrorMissingContinueFormatItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} cerror call(s) with no continue-format-control",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
