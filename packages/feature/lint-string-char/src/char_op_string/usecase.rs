//! Character-function-on-a-string (`(char= "a" c)`, `(char-code "x")` — a
//! guaranteed type error) detection across explicit files.

pub use crate::char_op_string::domain::{CharOpStringItem, build_char_op_string_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A character function given a string
/// is a guaranteed type error, but it is a build-breaking one only in a project
/// that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<CharOpStringItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} character function(s) given a non-character",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
