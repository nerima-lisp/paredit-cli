//! list* with nil tail ((list* a b nil) is (list a b)) detection.

pub use crate::list_star_nil::domain::{ListStarNilItem, build_list_star_nil_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A spelled-out `list` is noise, but it
/// is a build-breaking defect only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ListStarNilItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} nil-tailed list* call(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
