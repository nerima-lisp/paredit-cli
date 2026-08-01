//! Conflicting `into` accumulator detection across explicit files.

pub use crate::loop_into_accumulator_kind_conflict::domain::{
    LoopAccumulatorConflictItem, build_loop_accumulator_conflict_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// A kind conflict is a macroexpansion error in every implementation, but
/// whether it breaks *this* build stays the project's decision, as it is for
/// every other report in the suite.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<LoopAccumulatorConflictItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} conflicting loop accumulator(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
