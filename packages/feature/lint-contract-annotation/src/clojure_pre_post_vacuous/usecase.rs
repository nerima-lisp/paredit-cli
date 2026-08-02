//! Vacuous `:pre`/`:post` contract detection across explicit files.

pub use crate::clojure_pre_post_vacuous::domain::{
    ClojurePrePostVacuousItem, VacuousShape, build_clojure_pre_post_vacuous_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A vacuous contract is noise, but it
/// is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ClojurePrePostVacuousItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} vacuous :pre/:post contract(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
