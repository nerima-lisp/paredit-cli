//! Over-nested anonymous lambda chains across explicit files.

pub use crate::deeply_nested_anonymous_lambda::domain::{
    DeeplyNestedLambdaItem, build_deeply_nested_anonymous_lambda_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A deep lambda chain is a readability
/// judgement, and it is a build-breaking one only in a project that has decided
/// it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DeeplyNestedLambdaItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} deeply nested anonymous lambda chain(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
