//! inspect cohesion reporting across a set of files.

pub use crate::cohesion_report::domain::{DefinitionCoupling, build_cohesion_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every definition is listed with its coupling, but only an isolated one
/// can fail a build.
#[must_use]
pub fn evaluate_fail_on_isolated_policy(
    fail_on_isolated: bool,
    reports: &[FileFindings<DefinitionCoupling>],
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
                .filter(|coupling| coupling.isolated)
                .cloned()
                .collect(),
            summary: report.summary.clone(),
        })
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_isolated.then_some("--fail-on-isolated"),
        &failing,
        |report| {
            format!(
                "{} has {} isolated definition(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    );
    // The headline count stays the number of definitions measured; only
    // the gate narrows.
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
