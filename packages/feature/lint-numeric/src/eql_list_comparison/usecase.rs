//! `eq`/`eql`-on-a-quoted-list detection across explicit files.

pub use crate::eql_list_comparison::domain::{
    EqlListComparisonItem, build_eql_list_comparison_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Comparing against a quoted list with
/// `eq`/`eql` is a defect, but it is a build-breaking one only in a project
/// that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<EqlListComparisonItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} quoted-list identity comparison(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
