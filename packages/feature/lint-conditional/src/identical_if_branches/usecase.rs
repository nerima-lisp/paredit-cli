//! Identical `if`-branch (`(if c a a)`) detection across explicit files.

pub use crate::identical_if_branches::domain::{
    IdenticalIfBranchItem, build_identical_if_branch_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Two identical branches make the test
/// dead, but it is a build-breaking defect only in a project that has decided
/// it is.
#[must_use]
pub fn evaluate_fail_on_identical_policy(
    fail_on_identical: bool,
    reports: &[FileFindings<IdenticalIfBranchItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_identical.then_some("--fail-on-identical"),
        reports,
        |report| {
            format!(
                "{} has {} if form(s) with identical branches",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
