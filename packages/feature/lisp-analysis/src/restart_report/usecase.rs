//! inspect restarts reporting across a set of files.

pub use crate::restart_report::domain::{RestartFinding, build_restart_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every finding is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_unpaired_policy(
    fail_on_unpaired: bool,
    reports: &[FileFindings<RestartFinding>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| report.retained(|finding| finding.role.is_unpaired()))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_unpaired.then_some("--fail-on-unpaired"),
        &failing,
        |report| {
            format!(
                "{} has {} unpaired restart(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    );
    // The headline count stays the number of findings reported; only the
    // gate narrows.
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
