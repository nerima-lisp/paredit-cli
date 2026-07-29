//! Redundant `:test #'eql` (`(find x list :test #'eql)` is `(find x list)`)
//! detection across explicit files.

pub use crate::redundant_eql_test::domain::{
    RedundantEqlTestItem, build_redundant_eql_test_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Restating a default is noise, but it
/// is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RedundantEqlTestItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant :test #'eql argument(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
