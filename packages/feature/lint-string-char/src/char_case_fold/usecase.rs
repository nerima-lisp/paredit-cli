//! Char-case-fold ((char= (char-downcase a) (char-downcase b)) is (char-equal a b)) detection.

pub use crate::char_case_fold::domain::{CharCaseFoldItem, build_char_case_fold_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A case-folded `char=` is dead work,
/// but it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<CharCaseFoldItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} case-folded char= comparison(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
