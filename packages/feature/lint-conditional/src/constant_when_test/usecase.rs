//! Constant-test `when`/`unless` (`(when t …)` is `(progn …)`) detection.

pub use crate::constant_when_test::domain::{
    ConstantWhenTestItem, build_constant_when_test_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A statically decided branch is a
/// defect, but it is a build-breaking one only in a project that has decided it
/// is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ConstantWhenTestItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} constant when/unless test(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
