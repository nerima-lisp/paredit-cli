//! Misplaced-`loop`-clause detection across explicit files.

pub use crate::loop_clause_order_violation::domain::{
    ClauseOrderProblem, LoopClauseOrderItem, build_loop_clause_order_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, matching every other report in the
/// suite: a misplaced clause is a compile-time error in the loop macro, but
/// whether it breaks *this* build is the project's decision.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<LoopClauseOrderItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} misplaced loop clause(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
