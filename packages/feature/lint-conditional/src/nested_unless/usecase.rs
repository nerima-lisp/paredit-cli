//! Nested-`unless` (`(unless a (unless b body))` is `(unless (or a b) body)`)
//! detection across explicit files.

pub use crate::nested_unless::domain::{NestedUnlessItem, build_nested_unless_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A nested `unless` is a readability
/// defect, but it is a build-breaking one only in a project that has decided it
/// is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NestedUnlessItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} nested unless form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
