//! Constant-`if`-test (`(if t a b)` is `a`, `(if nil a b)` is `b`) detection
//! across explicit files.

pub use crate::constant_if_test::domain::{ConstantIfTestItem, build_constant_if_test_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A constant test is a defect, but it
/// is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ConstantIfTestItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} constant if test(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
