//! Over-long unlabelled literal argument lists across explicit files.

pub use crate::positional_argument_count_exceeds_readability::domain::{
    PositionalLiteralCallItem, build_positional_argument_count_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. An unlabelled argument list is a
/// readability judgement, and it is a build-breaking one only in a project that
/// has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<PositionalLiteralCallItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} over-long positional literal call(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
