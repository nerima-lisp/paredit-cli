//! A `dotimes`/`dolist` iteration variable assigned inside the body, across explicit files.

pub use crate::dotimes_dolist_index_var_mutated::domain::{
    DotimesDolistIndexVarMutatedItem, build_dotimes_dolist_index_var_mutated_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, like every other report in this
/// package: what this rule reports is a defect, but a build-breaking one only
/// in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DotimesDolistIndexVarMutatedItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} mutated iteration variable(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
