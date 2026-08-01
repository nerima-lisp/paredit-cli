//! Ineffective `dotimes` bound-mutation detection across explicit files.

pub use crate::dotimes_bound_mutation_has_no_effect::domain::{
    DotimesBoundMutationItem, build_dotimes_bound_mutation_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. The assignment is legal code whose
/// effect is merely not the one intended, so whether it fails a build is the
/// project's call.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DotimesBoundMutationItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} ineffective dotimes bound mutation(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
