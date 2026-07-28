//! inspect read-conditionals reporting across a set of files.

pub use crate::read_conditional_report::domain::{ReadConditional, build_read_conditional_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A reader conditional is a fact about the file,
/// not a defect by definition — it is a failure only in a project that has
/// decided it is one.
#[must_use]
pub fn evaluate_fail_on_conditional_policy(
    fail_on_conditional: bool,
    reports: &[FileFindings<ReadConditional>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_conditional.then_some("--fail-on-conditional"),
        reports,
        |report| {
            format!(
                "{} has {} reader conditional(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
