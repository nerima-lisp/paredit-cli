//! Redundant-`apply` (`(apply #'f (list a b))`, which is just `(f a b)`)
//! detection across explicit files.

pub use crate::redundant_apply::domain::{RedundantApplyItem, build_redundant_apply_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. An `apply` over a literal list is
/// ceremony, but it is a build-breaking one only in a project that has decided
/// it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RedundantApplyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant apply form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
