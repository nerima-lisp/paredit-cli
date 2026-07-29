//! Nested-boolean (`(or a (or b c))` is `(or a b c)`) detection across explicit
//! files.

pub use crate::nested_boolean::domain::{NestedBooleanItem, build_nested_boolean_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Redundant nesting is structure noise
/// that changes nothing about what the form computes, so it is a build-breaking
/// defect only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NestedBooleanItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} nested same-operator boolean(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
