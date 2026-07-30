//! Duplicate `and`/`or` operand detection across explicit files.

pub use crate::duplicate_boolean_operands::domain::{
    DuplicateBooleanOperandItem, build_duplicate_boolean_operand_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A repeated operand is a defect, but
/// it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_duplicate_policy(
    fail_on_duplicate: bool,
    reports: &[FileFindings<DuplicateBooleanOperandItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_duplicate.then_some("--fail-on-duplicate"),
        reports,
        |report| {
            format!(
                "{} has {} duplicated boolean operand(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
