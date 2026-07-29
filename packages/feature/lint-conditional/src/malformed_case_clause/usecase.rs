//! Malformed-`case`-family-clause (a clause that is not a non-empty list)
//! detection across explicit files.

pub use crate::malformed_case_clause::domain::{
    MalformedCaseClauseItem, build_malformed_case_clause_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A malformed clause is a program
/// error, but it is a build-breaking one only in a project that has decided
/// it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<MalformedCaseClauseItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} malformed case clause(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
