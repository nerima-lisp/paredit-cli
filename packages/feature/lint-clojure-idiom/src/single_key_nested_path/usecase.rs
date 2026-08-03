//! `single-key-nested-path` detection across explicit files.

pub use crate::single_key_nested_path::domain::{
    SingleKeyNestedPathItem, build_single_key_nested_path_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SingleKeyNestedPathItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} nested-path call(s) whose path holds a single key",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
