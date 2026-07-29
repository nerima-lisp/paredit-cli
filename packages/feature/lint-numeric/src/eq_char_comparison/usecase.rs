//! `eq`-on-a-character (`(eq c #\a)` — unreliable identity on characters)
//! detection across explicit files.

pub use crate::eq_char_comparison::domain::{
    EqCharComparisonItem, build_eq_char_comparison_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. `eq` on a character is a defect, but
/// it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<EqCharComparisonItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} eq-on-a-character comparison(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
