//! Redundant-fresh-line (`~%~&`, whose `~&` cannot emit) detection.

pub use crate::format_percent_ampersand_adjacent_redundancy::domain::{
    FormatPercentAmpersandAdjacentRedundancyItem,
    build_format_percent_ampersand_adjacent_redundancy_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Nothing here is undefined
/// behaviour: the string merely does not say what it appears to say, which
/// is a build-breaking defect only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<FormatPercentAmpersandAdjacentRedundancyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} control string(s) with a redundant ~& after a ~%",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
