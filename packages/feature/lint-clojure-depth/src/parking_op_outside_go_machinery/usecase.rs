//! `parking-op-outside-go-machinery` detection across explicit files.

pub use crate::parking_op_outside_go_machinery::domain::{
    ParkingOpOutsideGoMachineryItem, build_parking_op_outside_go_machinery_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ParkingOpOutsideGoMachineryItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} parking channel operation(s) the go transform cannot reach",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
