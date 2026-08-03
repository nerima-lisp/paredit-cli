//! `with-open-returns-lazy-seq` detection across explicit files.

pub use crate::with_open_returns_lazy_seq::domain::{
    WithOpenLazySeqItem, build_with_open_returns_lazy_seq_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<WithOpenLazySeqItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} with-open form(s) whose value is a lazy sequence over the resource they close",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
