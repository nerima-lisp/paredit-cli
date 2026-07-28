//! Change-frequency reporting across a set of files.

pub use crate::hotspot_report::domain::{
    Churn, Hotspot, build_hotspot_report, churn_target, measure_churn,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// The threshold is a score rather than a flag, because every file has a
/// hotspot score and only an unusually high one means anything. A caller that
/// armed this with "any finding" would be failing on the existence of code.
#[must_use]
pub fn evaluate_hotspot_policy(
    max_score: Option<usize>,
    reports: &[FileFindings<Hotspot>],
) -> ReportPolicy {
    let Some(limit) = max_score else {
        return ReportPolicy::fail_on_any(None, reports, |report| {
            report.path.display().to_string()
        });
    };

    let failing = reports
        .iter()
        .map(|report| FileFindings {
            path: report.path.clone(),
            dialect: report.dialect,
            dialect_modelled: report.dialect_modelled,
            findings: report
                .findings
                .iter()
                .filter(|hotspot| hotspot.score > limit)
                .cloned()
                .collect(),
            summary: report.summary.clone(),
        })
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(Some("--max-score"), &failing, |report| {
        format!(
            "{} has {} definition(s) above the hotspot threshold",
            report.path.display(),
            report.findings.len()
        )
    });
    // The headline count stays the number of definitions ranked; only the gate
    // narrows to the ones over the line.
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
