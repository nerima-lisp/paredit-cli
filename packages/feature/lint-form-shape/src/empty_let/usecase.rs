//! Empty-`let` (`(let () body)` is `(progn body)`) detection across explicit
//! files.

pub use crate::empty_let::domain::{EmptyLetItem, build_empty_let_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. An empty `let` is noise, but it is a
/// build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<EmptyLetItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} empty let(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
