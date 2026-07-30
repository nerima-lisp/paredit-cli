//! Make-hash-table-test ((make-hash-table :test 'eql) is (make-hash-table)) detection.

pub use crate::make_hash_table_test::domain::{
    MakeHashTableTestItem, build_make_hash_table_test_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A restated default is noise, but it
/// is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<MakeHashTableTestItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant make-hash-table :test 'eql",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
