//! Binds-constant (a let/let*/do/do* binding of nil, t, or a keyword) detection
//! across explicit files.

pub use crate::binds_constant::domain::{BindsConstantItem, build_binds_constant_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Binding a constant is a program
/// error, but it is a build-breaking one only in a project that has decided it
/// is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<BindsConstantItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} constant binding(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
