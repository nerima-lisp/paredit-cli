//! Empty-body (`(when x)`, `(dolist (x l))` — the test/spec runs then nothing
//! happens) detection across explicit files.

pub use crate::empty_body::domain::{EmptyBodyItem, build_empty_body_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A body-less `when` is almost always a
/// forgotten body, but it is a build-breaking defect only in a project that has
/// decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<EmptyBodyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} body-less form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
