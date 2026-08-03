//! Nested-`get` chain ((get (get m :a) :b) is (get-in m [:a :b])) detection.

pub use crate::nested_get_chain::domain::{GetChainItem, collect_get_chains};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A nested `get` is the long spelling
/// of a path lookup, not a wrong program, so only a project that has decided it
/// is may break its build on one.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<GetChainItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} nested get chain(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
