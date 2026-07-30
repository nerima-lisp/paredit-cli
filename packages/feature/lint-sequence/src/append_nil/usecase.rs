//! Append-nil ((append x nil) is (copy-list x)) detection.

pub use crate::append_nil::domain::{AppendNilItem, collect_append_nils};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. An `append` with a `nil` tail is a
/// roundabout `copy-list`, not a wrong program, so only a project that has
/// decided it is may break its build on one.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<AppendNilItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} append(s) with a nil tail",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
