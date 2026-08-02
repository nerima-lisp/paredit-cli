//! Redundant `(into [] coll)` conversion detection.

pub use crate::redundant_into_empty_collection::domain::{
    Conversion, RedundantIntoItem, collect_redundant_intos,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. `(into [] coll)` is the long spelling
/// of `(vec coll)`, not a wrong program, so only a project that has decided it
/// is may break its build on one.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RedundantIntoItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} into(s) onto an empty collection",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
