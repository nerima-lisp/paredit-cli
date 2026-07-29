//! `if`-arity (wrong argument count for the `if` special form) detection across
//! explicit files.

pub use crate::if_arity::domain::{IfArityItem, build_if_arity_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A misarity `if` is a program error,
/// but it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<IfArityItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} misarity if form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
