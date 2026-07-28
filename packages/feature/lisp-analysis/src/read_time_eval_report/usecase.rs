//! inspect read-time-eval reporting across a set of files.

pub use crate::read_time_eval_report::domain::{ReadTimeEval, build_read_time_eval_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A `#.` dispatch is a fact about the file,
/// not a defect by definition — it is a failure only in a project that has
/// decided it is one.
#[must_use]
pub fn evaluate_fail_on_read_eval_policy(
    fail_on_read_eval: bool,
    reports: &[FileFindings<ReadTimeEval>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_read_eval.then_some("--fail-on-read-eval"),
        reports,
        |report| {
            format!(
                "{} evaluates {} form(s) at read time",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
