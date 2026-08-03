//! Dead named-`let` detection across explicit files.

pub use crate::named_let_never_recurs::domain::{
    NamedLetNeverRecursItem, build_named_let_never_recurs_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NamedLetNeverRecursItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} named let(s) that never recur",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
