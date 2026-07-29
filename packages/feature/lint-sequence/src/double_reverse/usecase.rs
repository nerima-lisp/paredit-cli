//! Double-reverse ((reverse (reverse x)) is (copy-seq x)) detection.

pub use crate::double_reverse::domain::{DoubleReverseItem, build_double_reverse_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A double `reverse` is wasteful, but
/// it is a build-breaking defect only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DoubleReverseItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} double reverse(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
