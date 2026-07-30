//! Redundant-boolean-identity (`t` in `and`, `nil` in `or`, e.g. `(and a t b)`
//! is `(and a b)`) detection across explicit files.

pub use crate::redundant_boolean_identity::domain::{
    RedundantBooleanIdentityItem, build_redundant_boolean_identity_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A redundant identity operand is
/// clutter, but it is a build-breaking one only in a project that has decided
/// it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RedundantBooleanIdentityItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant boolean identity operand(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
