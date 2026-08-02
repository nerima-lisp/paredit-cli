//! Mixed-float-precision ((* 3.14 1.0d0) caps a double result at single
//! precision) detection.

pub use crate::mixed_float_precision_arithmetic::domain::{
    MixedFloatPrecisionItem, build_mixed_float_precision_arithmetic_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Which of the two precisions was
/// intended is the author's call — widening the literal changes every computed
/// result downstream — so whether a mixed form is build-breaking is a project's
/// decision rather than this rule's.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<MixedFloatPrecisionItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} mixed-float-precision form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
