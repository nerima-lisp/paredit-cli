//! inspect duplication-ratio reporting across a set of files.

pub use crate::duplication_ratio_report::domain::{RepeatedShape, build_duplication_ratio_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A repeated shape is a fact about the file,
/// not a defect by definition — it is a failure only in a project that has
/// decided it is one.
#[must_use]
pub fn evaluate_fail_on_duplication_policy(
    fail_on_duplication: bool,
    reports: &[FileFindings<RepeatedShape>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_duplication.then_some("--fail-on-duplication"),
        reports,
        |report| {
            format!(
                "{} repeats {} structural shape(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
