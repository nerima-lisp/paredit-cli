//! inspect macro-expansion reporting across a set of files.

pub use crate::macro_expansion_report::domain::{Expansion, build_macro_expansion_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every finding is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_declined_policy(
    fail_on_declined: bool,
    reports: &[FileFindings<Expansion>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| report.retained(|finding| finding.declined.is_some()))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_declined.then_some("--fail-on-declined"),
        &failing,
        |report| {
            format!(
                "{} has {} call site(s) this analysis declined to expand",
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
