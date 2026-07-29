//! Subseq-zero ((subseq x 0) is (copy-seq x)) detection.

pub use crate::subseq_zero::domain::{SubseqZeroItem, build_subseq_zero_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A `(subseq seq 0)` is correct code
/// that states a copy as bounds arithmetic, so failing a build over it is a
/// house-style decision.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SubseqZeroItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} subseq(s) from index 0",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
