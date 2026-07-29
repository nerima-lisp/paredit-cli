//! Nested-`cXr` (`(car (cdr x))`, better written `(cadr x)`) detection across
//! explicit files.

pub use crate::nested_cxr::domain::{NestedCxrItem, build_nested_cxr_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A nesting the standard already names
/// is worth collapsing, but it is a build-breaking one only in a project that
/// has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NestedCxrItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} combinable nested cXr accessor(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
