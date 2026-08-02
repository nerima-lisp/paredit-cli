//! Flattenable nested-`cond` detection across explicit files.

pub use crate::nested_cond_flattenable::domain::{
    NestedCondItem, build_nested_cond_flattenable_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A flattenable nested `cond` computes
/// exactly what its flat form computes, so it is a build-breaking defect only
/// in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NestedCondItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} flattenable nested cond form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
