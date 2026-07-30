//! Setq-non-variable (a setq/psetq place that is not a variable) detection
//! across explicit files.

pub use crate::setq_non_variable::domain::{SetqNonVariableItem, build_setq_non_variable_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. An invalid `setq` place is always a
/// program error, but it is a build-breaking one only in a project that has
/// decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SetqNonVariableItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} non-variable setq place(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
