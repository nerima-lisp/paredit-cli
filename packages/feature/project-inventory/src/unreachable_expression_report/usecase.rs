//! inspect unreachable-expressions reporting across a set of files.

pub use crate::unreachable_expression_report::domain::{
    UnreachableExpression, build_unreachable_expression_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A unreachable form is a fact about the file,
/// not a defect by definition — it is a failure only in a project that has
/// decided it is one.
#[must_use]
pub fn evaluate_fail_on_unreachable_policy(
    fail_on_unreachable: bool,
    reports: &[FileFindings<UnreachableExpression>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_unreachable.then_some("--fail-on-unreachable"),
        reports,
        |report| {
            format!(
                "{} has {} form(s) that cannot run",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
