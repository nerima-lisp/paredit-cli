//! Manual-`push` (`(setf x (cons item x))`, better written `(push item x)`)
//! detection across explicit files.

pub use crate::manual_push::domain::{ManualPushItem, build_manual_push_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A hand-written push is correct code
/// that states its intent indirectly, so it is build-breaking only in a project
/// that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ManualPushItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} manual push(es)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
