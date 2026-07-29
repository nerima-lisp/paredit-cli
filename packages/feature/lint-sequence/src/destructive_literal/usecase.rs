//! Destructive-operation-on-a-literal (`(nreverse '(a b))`, `(sort '(1 2) …)` —
//! undefined behavior) detection across explicit files.

pub use crate::destructive_literal::domain::{
    DestructiveLiteralItem, collect_destructive_literals,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Modifying a quoted literal is
/// undefined behavior, but it is a build-breaking one only in a project that
/// has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DestructiveLiteralItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} destructive call(s) on a quoted literal",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
