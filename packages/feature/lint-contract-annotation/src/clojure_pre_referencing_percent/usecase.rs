//! `%`-in-`:pre` detection across explicit files.

pub use crate::clojure_pre_referencing_percent::domain::{
    ClojurePreReferencingPercentItem, build_clojure_pre_referencing_percent_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ClojurePreReferencingPercentItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} :pre condition(s) naming %",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
