//! Duplicate-lambda-list-keyword (a lambda list repeating `&optional`/`&key`/…)
//! detection across explicit files.

pub use crate::duplicate_lambda_list_keyword::domain::{
    DuplicateLambdaListKeywordItem, build_duplicate_lambda_list_keyword_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A repeated lambda-list keyword is a
/// program error, but whether that stops a build is the project's call.
#[must_use]
pub fn evaluate_fail_on_duplicate_policy(
    fail_on_duplicate: bool,
    reports: &[FileFindings<DuplicateLambdaListKeywordItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_duplicate.then_some("--fail-on-duplicate"),
        reports,
        |report| {
            format!(
                "{} has {} repeated lambda-list keyword(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
