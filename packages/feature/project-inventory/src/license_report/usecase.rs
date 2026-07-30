//! inspect licenses reporting across a set of files.

pub use crate::license_report::domain::{Copyleft, SystemLicense, build_license_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every entry is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_review_policy(
    fail_on_review: bool,
    reports: &[FileFindings<SystemLicense>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| report.retained(|system| system.copyleft.needs_review()))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_review.then_some("--fail-on-review"),
        &failing,
        |report| {
            format!(
                "{} has {} system(s) whose licence needs review",
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
