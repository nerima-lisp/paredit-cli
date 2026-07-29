//! Nested-`progn` (a multi-form progn spliced into another progn) detection
//! across explicit files.

pub use crate::nested_progn::domain::{NestedPrognItem, build_nested_progn_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A nested progn is structure noise
/// that changes nothing about what the code does, so it is build-breaking only
/// in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NestedPrognItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} nested progn(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
