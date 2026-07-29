//! Sharp-quoted-`lambda` (`#'(lambda …)` is `(lambda …)`) detection across
//! explicit files.

pub use crate::sharp_quoted_lambda::domain::{
    SharpQuotedLambdaItem, build_sharp_quoted_lambda_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A redundant `#'` is noise, but it is
/// a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SharpQuotedLambdaItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} sharp-quoted lambda(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
