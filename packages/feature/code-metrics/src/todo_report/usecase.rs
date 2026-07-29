//! inspect todo reporting across a set of files.

pub use crate::todo_report::domain::{TaskMarker, build_todo_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A task marker is a fact about the file,
/// not a defect by definition — it is a failure only in a project that has
/// decided it is one.
#[must_use]
pub fn evaluate_fail_on_marker_policy(
    fail_on_marker: bool,
    reports: &[FileFindings<TaskMarker>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_marker.then_some("--fail-on-marker"),
        reports,
        |report| {
            format!(
                "{} has {} task marker(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
