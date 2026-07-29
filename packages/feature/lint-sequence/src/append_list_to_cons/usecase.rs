//! Append-list-to-cons ((append (list x) rest) is (cons x rest)) detection.

pub use crate::append_list_to_cons::domain::{AppendListToConsItem, collect_append_list_to_cons};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A one-element `append` is a
/// readability defect with an exact rewrite, not a wrong program, so only a
/// project that has decided it is may break its build on one.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<AppendListToConsItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} singleton append(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
