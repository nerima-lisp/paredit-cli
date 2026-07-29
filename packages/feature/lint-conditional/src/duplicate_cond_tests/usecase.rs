//! Duplicate `cond`-test detection across explicit files.

pub use crate::duplicate_cond_tests::domain::{
    DuplicateCondTestItem, build_duplicate_cond_test_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A repeated `cond` test is dead code,
/// but it is a build-breaking defect only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_duplicate_policy(
    fail_on_duplicate: bool,
    reports: &[FileFindings<DuplicateCondTestItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_duplicate.then_some("--fail-on-duplicate"),
        reports,
        |report| {
            format!(
                "{} has {} duplicated cond test(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
