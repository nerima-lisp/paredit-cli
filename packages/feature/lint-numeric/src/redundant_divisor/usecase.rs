//! Redundant-divisor ((floor x 1) is (floor x)) detection.

pub use crate::redundant_divisor::domain::{RedundantDivisorItem, build_redundant_divisor_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A unit divisor is correct code
/// carrying a redundant argument, so it is a build-breaking finding only in a
/// project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RedundantDivisorItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant divisor(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
