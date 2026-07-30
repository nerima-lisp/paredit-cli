//! `eq`/`eql`-on-a-string-literal detection across explicit files.

pub use crate::eql_string_comparison::domain::{
    EqlStringComparisonItem, build_eql_string_comparison_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Comparing against a string literal
/// with `eq`/`eql` is a defect, but it is a build-breaking one only in a
/// project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<EqlStringComparisonItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} string-literal identity comparison(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
