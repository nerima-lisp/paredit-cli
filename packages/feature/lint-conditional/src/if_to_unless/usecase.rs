//! If-to-unless ((if c nil e) is (unless c e)) detection.

pub use crate::if_to_unless::domain::{IfToUnlessItem, build_if_to_unless_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A `(if c nil e)` is correct code
/// spelled indirectly, so it is a build-breaking defect only in a project that
/// has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<IfToUnlessItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} if form(s) rewritable to unless",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
