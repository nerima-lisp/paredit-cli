//! Dead `loop` epilogue form detection across explicit files.

pub use crate::loop_unreachable_finally_clause::domain::{
    LoopUnreachableFinallyItem, build_loop_unreachable_finally_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Dead code compiles, so this gate is armed by a flag rather than always
/// on: a project decides for itself whether an unreachable epilogue form is
/// worth failing a build over.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<LoopUnreachableFinallyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} unreachable loop epilogue form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
