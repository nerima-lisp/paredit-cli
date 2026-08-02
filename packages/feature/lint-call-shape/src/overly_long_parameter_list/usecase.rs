//! Over-long required parameter lists across explicit files.

pub use crate::overly_long_parameter_list::domain::{
    LongParameterListItem, build_overly_long_parameter_list_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A long parameter list is a
/// readability judgement, and it is a build-breaking one only in a project that
/// has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<LongParameterListItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} over-long parameter list(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
