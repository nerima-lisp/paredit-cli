//! `inspect data-check` reporting across a set of files.

pub use crate::data_check_report::domain::{
    DataFormat, DataIssue, build_data_check_report, detect_data_format,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A duplicate key or a mismatched
/// tuple is a fact about the file, not a failure by definition — it is one
/// only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_finding_policy(
    fail_on_finding: bool,
    reports: &[FileFindings<DataIssue>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_finding.then_some("--fail-on-finding"),
        reports,
        |report| {
            format!(
                "{} has {} structural finding(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
