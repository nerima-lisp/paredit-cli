//! inspect circular-literals reporting across a set of files.

pub use crate::circular_literal_report::domain::{CircularLiteral, build_circular_literal_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A reader label is a fact about the file,
/// not a defect by definition — it is a failure only in a project that has
/// decided it is one.
#[must_use]
pub fn evaluate_fail_on_label_policy(
    fail_on_label: bool,
    reports: &[FileFindings<CircularLiteral>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_label.then_some("--fail-on-label"),
        reports,
        |report| {
            format!(
                "{} uses {} reader label dispatch(es)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
