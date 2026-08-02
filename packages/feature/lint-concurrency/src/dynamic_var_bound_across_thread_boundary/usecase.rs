//! `dynamic-var-bound-across-thread-boundary` detection across explicit files.

pub use crate::dynamic_var_bound_across_thread_boundary::domain::{
    DynamicVarBoundAcrossThreadBoundaryItem, build_dynamic_var_bound_across_thread_boundary_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DynamicVarBoundAcrossThreadBoundaryItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} thread body/bodies reading a special the enclosing let rebinds",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
