//! `reference-type-operator-mismatch` detection across explicit files.

pub use crate::reference_type_operator_mismatch::domain::{
    ReferenceTypeOperatorMismatchItem, build_reference_type_operator_mismatch_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ReferenceTypeOperatorMismatchItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} reference operator(s) applied to the wrong reference type",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
