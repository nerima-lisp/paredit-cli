//! inspect method-combination reporting across a set of files.

pub use crate::method_combination_report::domain::{
    MethodFinding, build_method_combination_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report: every
/// finding is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_orphaned_policy(
    fail_on_orphaned: bool,
    reports: &[FileFindings<MethodFinding>],
) -> ReportPolicy {
    // The gate fires on a subset of the findings, not on any finding at all:
    // an auxiliary method with no primary is a defect; an ordinary method is not.
    let failing = reports
        .iter()
        .map(|report| report.retained(|method| method.orphaned))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_orphaned.then_some("--fail-on-orphaned"),
        &failing,
        |report| {
            format!(
                "{} has {} auxiliary method(s) with no primary",
                report.path.display(),
                report.findings.len()
            )
        },
    );
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
