//! Lambda-list-keyword-order (keywords out of the canonical &optional/&rest/
//! &key/&allow-other-keys/&aux order) detection across explicit files.

pub use crate::lambda_list_keyword_order::domain::{
    LambdaListKeywordOrderItem, build_lambda_list_keyword_order_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A misordered lambda list is a program
/// error, but it is a build-breaking one only in a project that has decided it
/// is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<LambdaListKeywordOrderItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} misordered lambda list(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
