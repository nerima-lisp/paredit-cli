//! inspect use-widening reporting across a set of files.

pub use crate::use_widening_report::domain::{UseWideningItem, build_use_widening_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: every `:use`d package is listed,
/// but only `--fail-on-use` turns that list into a build failure.
#[must_use]
pub fn evaluate_fail_on_use_policy(
    fail_on_use: bool,
    reports: &[FileFindings<UseWideningItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(fail_on_use.then_some("--fail-on-use"), reports, |report| {
        format!(
            "{} has {} :use widening-risk finding(s)",
            report.path.display(),
            report.findings.len()
        )
    })
}
