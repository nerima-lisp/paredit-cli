//! Typep-predicate ((typep x 'string) is (stringp x)) detection.

pub use crate::typep_predicate::domain::{TypepPredicateItem, build_typep_predicate_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A `typep` with a dedicated predicate
/// is correct code, so failing a build over it is a house-style decision.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<TypepPredicateItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} typep(s) with a dedicated predicate",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
