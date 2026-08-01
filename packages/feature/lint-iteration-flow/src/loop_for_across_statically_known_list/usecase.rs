//! `loop for … across` over a provable list, across explicit files.

pub use crate::loop_for_across_statically_known_list::domain::{
    ListEvidence, LoopForAcrossListItem, build_loop_for_across_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, matching every other report in the
/// suite, even though this one finds a guaranteed run-time type error.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<LoopForAcrossListItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} loop across clause(s) over a list",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
