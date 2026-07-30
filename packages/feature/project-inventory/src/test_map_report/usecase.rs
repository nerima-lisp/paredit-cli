//! inspect test-map reporting across a set of files.

pub use crate::test_map_report::domain::{Coverage, CoverageEntry, build_test_map_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every entry is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_untested_policy(
    fail_on_untested: bool,
    reports: &[FileFindings<CoverageEntry>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| report.retained(|entry| entry.coverage == Coverage::Untested))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_untested.then_some("--fail-on-untested"),
        &failing,
        |report| {
            format!(
                "{} has {} untested definition(s)",
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
