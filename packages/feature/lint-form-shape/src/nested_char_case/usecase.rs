//! Nested char case ((char-upcase (char-downcase c)) is (char-upcase c)) detection.

pub use crate::nested_char_case::domain::{NestedCharCaseItem, build_nested_char_case_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A dead inner case op is waste, but it
/// is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NestedCharCaseItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} nested char case op(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
