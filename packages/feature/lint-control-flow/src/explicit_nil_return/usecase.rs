//! Explicit-nil-return (`(return nil)` is `(return)`) detection across explicit
//! files.

pub use crate::explicit_nil_return::domain::{
    ExplicitNilReturnItem, build_explicit_nil_return_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A redundant `nil` result changes
/// nothing about what the code does, so it is build-breaking only in a project
/// that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ExplicitNilReturnItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} explicit nil return(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
