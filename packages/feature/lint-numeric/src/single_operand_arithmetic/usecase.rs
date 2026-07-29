//! Single-operand-`+`/`*` (`(+ X)`/`(* X)`, which are just `X`) detection
//! across explicit files.

pub use crate::single_operand_arithmetic::domain::{
    SingleOperandArithmeticItem, build_single_operand_arithmetic_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A single-operand `+` is redundant
/// rather than wrong — it is often what a macro expansion legitimately
/// produced — so breaking the build on one is a project's decision.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SingleOperandArithmeticItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} single-operand arithmetic form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
