//! Manual-`pushnew` (`(setf x (adjoin item x))`, better written
//! `(pushnew item x)`) detection across explicit files.

pub use crate::manual_pushnew::domain::{ManualPushnewItem, build_manual_pushnew_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A hand-written pushnew is correct code
/// that states its intent indirectly, so it is build-breaking only in a project
/// that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ManualPushnewItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} manual pushnew(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
