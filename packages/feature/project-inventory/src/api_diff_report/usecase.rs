//! Public-API drift across a set of files.

pub use crate::api_diff_report::domain::{
    ApiChange, BaselineEntry, Impact, build_api_diff_report, read_baseline,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// The gate is the bump the caller *intends*: a release claiming to be a minor
/// version fails when the diff requires a major one. Naming the intended bump
/// rather than "fail on any change" is what makes this usable in CI, since
/// every release has changes.
#[must_use]
pub fn evaluate_api_diff_policy(
    intended_bump: Option<&str>,
    reports: &[FileFindings<ApiChange>],
) -> ReportPolicy {
    let Some(intended) = intended_bump else {
        return ReportPolicy::fail_on_any(None, reports, |report| {
            report.path.display().to_string()
        });
    };

    let permitted = match intended {
        "major" => Impact::Breaking,
        "minor" => Impact::Compatible,
        _ => Impact::Unchanged,
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
                .filter(|change| change.impact > permitted)
                .cloned()
                .collect(),
            summary: report.summary.clone(),
        })
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(Some("--intended-bump"), &failing, |report| {
        format!(
            "{} has {} change(s) requiring a larger version bump",
            report.path.display(),
            report.findings.len()
        )
    });
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
