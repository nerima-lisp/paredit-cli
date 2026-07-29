//! De Morgan (`(and (not a) (not b))` is `(not (or a b))`) detection across
//! explicit files.

pub use crate::de_morgan::domain::{DeMorganItem, build_de_morgan_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A De Morgan collapse is a readability
/// win, not a bug, so failing the build on one is a choice a project makes
/// rather than one this tool makes for it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DeMorganItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} De Morgan-collapsible boolean(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
