//! Redundant-precision-coercion ((truncate (coerce x 'double-float)) discards
//! the float it just built) detection.

pub use crate::redundant_precision_coercion::domain::{
    RedundantPrecisionCoercionItem, build_redundant_precision_coercion_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. The two forms are genuinely different
/// functions — dropping the coercion changes the result near an integer
/// boundary — so which one the author meant is a question this rule raises
/// rather than answers.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RedundantPrecisionCoercionItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} discarded float coercion(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
