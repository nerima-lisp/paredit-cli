//! Redundant-prog1 ((prog1 x) is x) detection.

pub use crate::redundant_prog1::domain::{RedundantProg1Item, build_redundant_prog1_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A single-form `prog1` is noise, but
/// it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RedundantProg1Item>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} single-form prog1 form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
