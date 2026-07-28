//! inspect indentation reporting across a set of files.

pub use crate::indentation_report::domain::{IndentFinding, build_indentation_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A indentation deviation is a fact about the file,
/// not a defect by definition — it is a failure only in a project that has
/// decided it is one.
#[must_use]
pub fn evaluate_fail_on_deviation_policy(
    fail_on_deviation: bool,
    reports: &[FileFindings<IndentFinding>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_deviation.then_some("--fail-on-deviation"),
        reports,
        |report| {
            format!(
                "{} has {} form(s) indented against the Emacs convention",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
