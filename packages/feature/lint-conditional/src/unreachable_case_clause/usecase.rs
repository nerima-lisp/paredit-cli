//! Unreachable-`case`/`typecase`-clause (clauses stranded after a `t`/`otherwise`
//! catch-all) detection across explicit files.

pub use crate::unreachable_case_clause::domain::{
    UnreachableCaseClauseItem, build_unreachable_case_clause_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A stranded clause is dead code, but
/// it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<UnreachableCaseClauseItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} case/typecase form(s) with unreachable clauses",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
