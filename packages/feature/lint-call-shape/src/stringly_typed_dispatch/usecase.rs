//! Stringly-typed dispatch across explicit files.

pub use crate::stringly_typed_dispatch::domain::{
    StringlyTypedDispatchItem, build_stringly_typed_dispatch_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Dispatching on strings is a
/// judgement, and it is a build-breaking one only in a project that has decided
/// it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<StringlyTypedDispatchItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} stringly-typed dispatch(es)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
