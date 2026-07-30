//! Malformed-`let`/`let*`-binding (wrong element count) detection across
//! explicit files.

pub use crate::malformed_let_binding::domain::{
    MalformedLetBindingItem, build_malformed_let_binding_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A malformed binding is a program
/// error, but it is a build-breaking one only in a project that has decided it
/// is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<MalformedLetBindingItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} malformed let binding(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
