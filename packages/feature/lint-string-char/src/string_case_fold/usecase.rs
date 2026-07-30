//! String-case-fold ((string= (string-downcase a) (string-downcase b)) is (string-equal a b)) detection.

pub use crate::string_case_fold::domain::{StringCaseFoldItem, build_string_case_fold_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A folded `string=` is correct code
/// that allocates two copies to say `string-equal`, so failing a build over it
/// is a house-style decision.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<StringCaseFoldItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} case-folded string= comparison(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
