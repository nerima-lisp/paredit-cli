//! `the`-arity (a `the` special form without exactly two arguments) detection
//! across explicit files.

pub use crate::the_arity::domain::{TheArityItem, build_the_arity_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A misarity `the` is a program error,
/// but it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<TheArityItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} misarity the form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
