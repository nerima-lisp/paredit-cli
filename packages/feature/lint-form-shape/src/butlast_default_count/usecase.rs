//! Redundant butlast count of 1 ((butlast x 1) is (butlast x)) detection.

pub use crate::butlast_default_count::domain::{
    ButlastDefaultCountItem, build_butlast_default_count_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A count argument that restates the
/// default is noise, but it is a build-breaking one only in a project that has
/// decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ButlastDefaultCountItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant butlast count(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
