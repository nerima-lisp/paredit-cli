//! Car-nthcdr ((car (nthcdr n x)) is (nth n x)) detection.

pub use crate::car_nthcdr::domain::{CarNthcdrItem, collect_car_nthcdrs};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A `car` of an `nthcdr` is the
/// long spelling of `nth`, not a wrong program, so only a project that has
/// decided it is may break its build on one.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<CarNthcdrItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} car(s) of an nthcdr",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
