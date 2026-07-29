//! Single-operand list-op (`(append x)` is `x`) detection across explicit
//! files.

pub use crate::single_operand_list_op::domain::{
    SingleOperandListOpItem, build_single_operand_list_op_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A single-argument `append` is correct
/// code that does nothing, so failing a build over it is a house-style
/// decision.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SingleOperandListOpItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} single-argument list op(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
