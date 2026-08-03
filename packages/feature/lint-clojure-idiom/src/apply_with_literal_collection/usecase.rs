//! `apply-with-literal-collection` detection across explicit files.

pub use crate::apply_with_literal_collection::domain::{
    ApplyWithLiteralCollectionItem, build_apply_with_literal_collection_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ApplyWithLiteralCollectionItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} apply call(s) whose argument sequence is a literal",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
