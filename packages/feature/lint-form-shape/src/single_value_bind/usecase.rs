//! Single-value `multiple-value-bind` (`(multiple-value-bind (x) f body)` is
//! `(let ((x f)) body)`) detection across explicit files.

pub use crate::single_value_bind::domain::{SingleValueBindItem, build_single_value_bind_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A single-value `multiple-value-bind`
/// is noise, but it is a build-breaking one only in a project that has decided
/// it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SingleValueBindItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} single-value multiple-value-bind(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
