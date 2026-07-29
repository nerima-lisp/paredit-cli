//! Format-to-string ((format nil "~A" x) is (princ-to-string x)) detection.

pub use crate::format_to_string::domain::{FormatToStringItem, build_format_to_string_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A single-directive
/// `(format nil …)` re-parses a control string at run time for nothing, but it
/// is a build-breaking defect only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<FormatToStringItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} format-to-string call(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
