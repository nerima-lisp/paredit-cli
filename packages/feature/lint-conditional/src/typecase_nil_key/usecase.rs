//! Typecase-`nil`-key (`(typecase x (nil …))` is a dead clause; use `(null …)`)
//! detection across explicit files.

pub use crate::typecase_nil_key::domain::{TypecaseNilKeyItem, build_typecase_nil_key_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A bare `nil` type specifier is a dead
/// clause, but it is a build-breaking one only in a project that has decided it
/// is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<TypecaseNilKeyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} bare nil typecase key(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
