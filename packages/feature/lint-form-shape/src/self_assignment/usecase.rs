//! Self-assignment (`(setq x x)`) detection across explicit files.

pub use crate::self_assignment::domain::{SelfAssignmentItem, build_self_assignment_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A self-assignment is almost always a
/// typo, but it is a build-breaking one only in a project that has decided it
/// is.
#[must_use]
pub fn evaluate_fail_on_self_assignment_policy(
    fail_on_self_assignment: bool,
    reports: &[FileFindings<SelfAssignmentItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_self_assignment.then_some("--fail-on-self-assignment"),
        reports,
        |report| {
            format!(
                "{} has {} self-assignment(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
