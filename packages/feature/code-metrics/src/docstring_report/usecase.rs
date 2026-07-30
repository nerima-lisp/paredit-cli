//! inspect docstrings reporting across a set of files.

pub use crate::docstring_report::domain::{
    DocstringFinding, DocstringIssue, build_docstring_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every definition is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_defect_policy(
    fail_on_defect: bool,
    reports: &[FileFindings<DocstringFinding>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| report.retained(|finding| finding.issue.is_defect()))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_defect.then_some("--fail-on-defect"),
        &failing,
        |report| {
            format!(
                "{} has {} documentation defect(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    );
    // The headline count stays the number of definitions reported; only the
    // gate narrows.
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
