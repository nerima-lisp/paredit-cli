//! `unsynchronized-shared-mutation` detection across explicit files.

pub use crate::unsynchronized_shared_mutation::domain::{
    UnsynchronizedSharedMutationItem, build_unsynchronized_shared_mutation_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<UnsynchronizedSharedMutationItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} unsynchronized mutation(s) of a global inside a thread body",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
