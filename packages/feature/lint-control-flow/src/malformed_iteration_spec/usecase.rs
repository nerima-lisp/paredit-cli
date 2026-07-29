//! Malformed-iteration-spec (a `dolist`/`dotimes` spec that is not
//! `(var form [result])`) detection across explicit files.

pub use crate::malformed_iteration_spec::domain::{
    MalformedIterationSpecItem, build_malformed_iteration_spec_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A malformed spec is a program error
/// caught at macroexpansion, but it is a build-breaking one only in a project
/// that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<MalformedIterationSpecItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} malformed iteration spec(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
