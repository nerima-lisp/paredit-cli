//! Negated-`when`/`unless` (a `(not X)`/`(null X)` test) detection across
//! explicit files.

pub use crate::negated_when_unless::domain::{
    NegatedWhenUnlessItem, build_negated_when_unless_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A negated `when`/`unless` is correct
/// code spelled as a double negative, so it is a build-breaking defect only in
/// a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NegatedWhenUnlessItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} negated when/unless form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
