//! Redundant :count nil ((remove x seq :count nil) is (remove x seq)) detection.

pub use crate::redundant_count_nil::domain::{
    RedundantCountNilItem, build_redundant_count_nil_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Restating a default is noise, but it
/// is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RedundantCountNilItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant :count nil argument(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
