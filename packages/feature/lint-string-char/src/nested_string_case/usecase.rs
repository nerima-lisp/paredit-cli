//! Nested string-case (`(OUTER (INNER s))` of two non-destructive string case
//! operations is `(OUTER s)`) detection across explicit files.

pub use crate::nested_string_case::domain::{
    NestedStringCaseItem, build_nested_string_case_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. The inner case operation is dead
/// work, but it is a build-breaking defect only in a project that has decided
/// it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NestedStringCaseItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} nested string case op pair(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
