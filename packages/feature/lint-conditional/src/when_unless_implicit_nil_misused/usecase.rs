//! Implicit-`nil` misuse detection across explicit files.

pub use crate::when_unless_implicit_nil_misused::domain::{
    ImplicitNilItem, build_when_unless_implicit_nil_misused_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, like every other report in this
/// package — though this one reports a `type-error` waiting to happen rather
/// than a style preference.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ImplicitNilItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} when/unless value(s) reaching a numeric operator",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
