//! Long-literal membership-test detection.

pub use crate::set_membership_via_linear_scan::domain::{LinearScanItem, collect_linear_scans};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A long `member` list is slower and
/// less clear than the set it stands for, not a wrong program, so only a
/// project that has decided otherwise may break its build on one.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<LinearScanItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} linear membership scan(s) over a literal set",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
