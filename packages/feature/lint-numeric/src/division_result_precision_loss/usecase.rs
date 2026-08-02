//! Division-result-precision-loss (Emacs Lisp `(/ 1 3)` is `0`) detection.

pub use crate::division_result_precision_loss::domain::{
    DivisionPrecisionLossItem, build_division_result_precision_loss_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Whether the author wanted a float
/// division or genuinely wanted `0` is not something this rule can decide, so
/// whether it is build-breaking is a project's call.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DivisionPrecisionLossItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} value-discarding integer division(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
