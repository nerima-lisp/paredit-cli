//! inspect external-systems reporting across a set of files.

pub use crate::external_system_report::domain::{SystemDependency, build_external_system_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every entry is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_external_policy(
    fail_on_external: bool,
    reports: &[FileFindings<SystemDependency>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| report.retained(|dependency| !dependency.internal))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_external.then_some("--fail-on-external"),
        &failing,
        |report| {
            format!(
                "{} depends on {} system(s) it does not define",
                report.path.display(),
                report.findings.len()
            )
        },
    );
    // The headline count stays the number of entries reported; only
    // the gate narrows.
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
