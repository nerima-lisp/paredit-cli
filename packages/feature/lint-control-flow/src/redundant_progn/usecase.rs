//! Redundant-`progn` (empty, or wrapping a single form) detection across
//! explicit files.

pub use crate::redundant_progn::domain::{RedundantPrognItem, build_redundant_progn_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A redundant progn is noise, but it is
/// a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RedundantPrognItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant progn(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
