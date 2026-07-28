//! inspect keyword-arity reporting across a set of files.

pub use crate::keyword_arity_report::domain::{ArityFinding, build_keyword_arity_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A arity fault is a fact about the file,
/// not a defect by definition — it is a failure only in a project that has
/// decided it is one.
#[must_use]
pub fn evaluate_fail_on_fault_policy(
    fail_on_fault: bool,
    reports: &[FileFindings<ArityFinding>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_fault.then_some("--fail-on-fault"),
        reports,
        |report| {
            format!(
                "{} has {} call(s) that do not fit their callee",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
