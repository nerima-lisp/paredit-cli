//! `return` with no enclosing form establishing the implicit `nil` block, across explicit files.

pub use crate::return_outside_implicit_nil_block::domain::{
    ReturnOutsideImplicitNilBlockItem, build_return_outside_implicit_nil_block_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, like every other report in this
/// package: what this rule reports is a defect, but a build-breaking one only
/// in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ReturnOutsideImplicitNilBlockItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} return(s) outside any implicit nil block",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
