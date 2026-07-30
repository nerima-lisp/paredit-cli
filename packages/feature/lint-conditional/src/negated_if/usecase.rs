//! Negated-`if` (`(if (not c) a b)`, better written `(if c b a)`) detection
//! across explicit files.

pub use crate::negated_if::domain::{NegatedIfItem, build_negated_if_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A negated `if` is correct code
/// spelled backwards, so it is a build-breaking defect only in a project that
/// has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NegatedIfItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} negated if form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
