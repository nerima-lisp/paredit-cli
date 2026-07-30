//! One-armed-`if` (`(if test then)`, better written `(when test then)`)
//! detection across explicit files.

pub use crate::one_armed_if::domain::{OneArmedIfItem, build_one_armed_if_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A one-armed `if` is a style defect,
/// but it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<OneArmedIfItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} one-armed if form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
