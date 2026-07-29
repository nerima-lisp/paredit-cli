//! Redundant-`let*` (`let*` with ≤ 1 binding is `let`) detection across
//! explicit files.

pub use crate::redundant_let_star::domain::{
    RedundantLetStarItem, build_redundant_let_star_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A `let*` that never uses sequential
/// scope makes a reader check for a dependency that is not there, but it is a
/// build-breaking defect only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RedundantLetStarItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant let* form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
