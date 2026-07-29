//! Case-`nil`-key (`(case x (nil …))` is a dead clause; use `((nil) …)`)
//! detection across explicit files.

pub use crate::case_nil_key::domain::{CaseNilKeyItem, build_case_nil_key_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A dead `case` clause is a defect, but
/// it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<CaseNilKeyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} bare nil case key(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
