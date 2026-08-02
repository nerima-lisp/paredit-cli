//! Epsilon-less float loop bound ((do ((x 0.0 (+ x 0.1))) ((= x 1))) never
//! terminates) detection.

pub use crate::epsilon_less_float_loop_bound::domain::{
    EpsilonLessLoopItem, build_epsilon_less_float_loop_bound_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Replacing `=` with `>=` changes which
/// iteration is the last one, so whether a drifting loop is build-breaking is a
/// project's call rather than this rule's.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<EpsilonLessLoopItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} equality-terminated float loop(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
