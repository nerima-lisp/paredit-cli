//! Funcall-of-lambda (`(funcall (lambda (x) …) a)` is `((lambda (x) …) a)`)
//! detection across explicit files.

pub use crate::funcall_lambda::domain::{FuncallLambdaItem, build_funcall_lambda_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A `funcall` of a literal lambda is
/// noise, but it is a build-breaking one only in a project that has decided it
/// is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<FuncallLambdaItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} funcall(s) of a lambda",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
