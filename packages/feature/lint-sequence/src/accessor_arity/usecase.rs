//! Accessor-arity (an nth/elt/gethash/getf/... accessor with the wrong argument
//! count) detection across explicit files.

pub use crate::accessor_arity::domain::{
    AccessorArityItem, collect_accessor_arity_violations, expected_arity_phrase,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A misarity accessor call is a
/// compile-time program error, but it is a build-breaking one only in a project
/// that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<AccessorArityItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} misarity accessor call(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
