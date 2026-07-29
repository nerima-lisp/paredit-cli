//! Zero-divisor ((/ x 0) is a division by zero) detection.

pub use crate::zero_divisor::domain::{ZeroDivisorItem, build_zero_divisor_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, for consistency with every other
/// report in this campaign — even though a literal-zero divisor is the one
/// finding here that cannot be anything but a bug, since the form can only ever
/// signal `division-by-zero`.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ZeroDivisorItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} literal-zero divisor(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
