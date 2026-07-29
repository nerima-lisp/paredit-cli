//! Identity-arithmetic (`(+ x 0)`, `(* x 1)` — a redundant identity operand)
//! detection across explicit files.

pub use crate::identity_arithmetic::domain::{
    IdentityArithmeticItem, build_identity_arithmetic_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A redundant identity operand is
/// noise, not a bug, so it breaks a build only in a project that has decided it
/// should.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<IdentityArithmeticItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant identity operand(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
