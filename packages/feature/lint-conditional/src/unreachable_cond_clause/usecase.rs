//! Unreachable-`cond`-clause (clauses stranded after a `t` catch-all) detection
//! across explicit files.

pub use crate::unreachable_cond_clause::domain::{
    UnreachableCondClauseItem, build_unreachable_cond_clause_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A stranded clause is dead code, but
/// it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<UnreachableCondClauseItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} cond form(s) with unreachable clauses",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
