//! Single-clause-`cond` (`(cond (test body))` is `(when test body)`) detection
//! across explicit files.

pub use crate::single_clause_cond::domain::{
    SingleClauseCondItem, build_single_clause_cond_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A single-clause `cond` is a
/// readability defect, but it is a build-breaking one only in a project that
/// has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SingleClauseCondItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} single-clause cond form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
