//! Definition attribution across a set of files.

pub use crate::blame_report::domain::{
    Attribution, Blame, LineBlame, build_blame_report, measure_blame,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Narrower than the report: every definition is listed, and only one git
/// could not attribute can fail a build. An attribution is not a defect.
#[must_use]
pub fn evaluate_blame_policy(
    fail_on_unattributed: bool,
    reports: &[FileFindings<Attribution>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| FileFindings {
            path: report.path.clone(),
            dialect: report.dialect,
            dialect_modelled: report.dialect_modelled,
            findings: report
                .findings
                .iter()
                .filter(|attribution| attribution.author.is_none())
                .cloned()
                .collect(),
            summary: report.summary.clone(),
        })
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_unattributed.then_some("--fail-on-unattributed"),
        &failing,
        |report| {
            format!(
                "{} has {} definition(s) git could not attribute",
                report.path.display(),
                report.findings.len()
            )
        },
    );
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
