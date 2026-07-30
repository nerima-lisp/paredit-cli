//! inspect readtable-case reporting across a set of files.

pub use crate::readtable_case_report::domain::{CaseSensitiveSymbol, build_readtable_case_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every finding is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_fragile_policy(
    fail_on_fragile: bool,
    reports: &[FileFindings<CaseSensitiveSymbol>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| report.retained(|finding| finding.sensitivity.is_fragile()))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_fragile.then_some("--fail-on-fragile"),
        &failing,
        |report| {
            format!(
                "{} has {} symbol(s) whose identity depends on readtable-case",
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
