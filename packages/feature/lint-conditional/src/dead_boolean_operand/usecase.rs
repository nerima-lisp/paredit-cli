//! Dead-boolean-operand (`(and a nil b)`, `(or a t b)`) detection across explicit files.

pub use crate::dead_boolean_operand::domain::{
    DeadBooleanOperandItem, build_dead_boolean_operand_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. An unreachable operand is a defect,
/// but it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DeadBooleanOperandItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} short-circuiting boolean(s) with dead operands",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
