//! `tagbody` labels no `go` in the form targets, across explicit files.

pub use crate::tagbody_unreachable_tag::domain::{
    TagbodyUnreachableTagItem, build_tagbody_unreachable_tag_report,
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
    reports: &[FileFindings<TagbodyUnreachableTagItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} unreachable tagbody tag(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
