//! `multiple-value-bind` forms whose body references none of the variables they bind, across explicit files.

pub use crate::multiple_value_bind_all_ignored::domain::{
    MultipleValueBindAllIgnoredItem, build_multiple_value_bind_all_ignored_report,
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
    reports: &[FileFindings<MultipleValueBindAllIgnoredItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} multiple-value-bind form(s) whose variables are all unused",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
