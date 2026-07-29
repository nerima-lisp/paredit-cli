//! If-to-`or` (`(if x x y)` is `(or x y)`) detection across explicit files.

pub use crate::if_to_or::domain::{IfToOrItem, build_if_to_or_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. `(if x x y)` evaluates `x` twice
/// where `or` evaluates it once, but it is a build-breaking defect only in a
/// project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<IfToOrItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} (if x x y) form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
