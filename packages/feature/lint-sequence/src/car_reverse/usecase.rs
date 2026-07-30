//! Car-reverse ((car (reverse x)) is (car (last x))) detection.

pub use crate::car_reverse::domain::{CarReverseItem, collect_car_reverses};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A `car` of a `reverse` costs a
/// whole copy to read one element, which is a performance defect rather than
/// a wrong program, so only a project that has decided it is may break its
/// build on one.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<CarReverseItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} car(s) of a reverse",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
