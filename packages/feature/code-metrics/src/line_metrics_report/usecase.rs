//! Line-shape reporting across a set of files.

pub use crate::line_metrics_report::domain::{
    LineFinding, LineThresholds, Overflow, build_line_metrics_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A threshold is a project's opinion,
/// not a property of the code, so exceeding one is only a failure where someone
/// decided it is.
#[must_use]
pub fn evaluate_line_metrics_policy(
    fail_on_overflow: bool,
    reports: &[FileFindings<LineFinding>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_overflow.then_some("--fail-on-overflow"),
        reports,
        |report| {
            format!(
                "{} exceeds {} threshold(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
