//! Nested-`when` (`(when a (when b body))` is `(when (and a b) body)`) detection
//! across explicit files.

pub use crate::nested_when::domain::{NestedWhenItem, build_nested_when_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A nested `when` is a readability
/// defect, but it is a build-breaking one only in a project that has decided it
/// is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NestedWhenItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} nested when form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
