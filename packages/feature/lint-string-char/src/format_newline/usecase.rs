//! Format-newline ((format t "~%") is (terpri)) detection.

pub use crate::format_newline::domain::{FormatNewlineItem, build_format_newline_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A `(format t "~%")` is a control
/// string parsed at run time for nothing, but it is a build-breaking defect
/// only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<FormatNewlineItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} (format t \"~%\") call(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
