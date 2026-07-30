//! inspect format-directives reporting across a set of files.

pub use crate::format_directive_report::domain::{
    FormatCall, Verdict, build_format_directive_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report: only a
/// provable mismatch fails a build. An indeterminate call is analyzed and
/// reported, but it is evidence of nothing.
#[must_use]
pub fn evaluate_fail_on_mismatch_policy(
    fail_on_mismatch: bool,
    reports: &[FileFindings<FormatCall>],
) -> ReportPolicy {
    // Unlike the other gates here, this one counts a *subset* of the findings:
    // an indeterminate call is analyzed and reported, but it is not evidence of
    // anything, so it must not fail a build.
    let mismatched = reports
        .iter()
        .map(|report| report.retained(|call| call.verdict.is_mismatch()))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_mismatch.then_some("--fail-on-mismatch"),
        &mismatched,
        |report| {
            format!(
                "{} has {} format call(s) whose arguments do not match the control string",
                report.path.display(),
                report.findings.len()
            )
        },
    );
    // The headline count stays the number of calls analyzed; only the gate
    // narrows to mismatches.
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
