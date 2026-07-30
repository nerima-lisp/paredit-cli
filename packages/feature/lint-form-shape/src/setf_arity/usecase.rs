//! Odd-arity `setf`/`setq` detection across explicit files.

pub use crate::setf_arity::domain::{SetfArityItem, build_setf_arity_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. An odd-arity assignment is always an
/// error, but it is a build-breaking one only in a project that has decided it
/// is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SetfArityItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} odd-arity assignment(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
