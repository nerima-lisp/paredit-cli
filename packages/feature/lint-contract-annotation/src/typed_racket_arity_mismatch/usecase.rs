//! Typed Racket annotation/definition arity-disagreement detection across
//! explicit files.

pub use crate::typed_racket_arity_mismatch::domain::{
    TypedRacketArityMismatchItem, build_typed_racket_arity_mismatch_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<TypedRacketArityMismatchItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} annotation/define arity disagreement(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
