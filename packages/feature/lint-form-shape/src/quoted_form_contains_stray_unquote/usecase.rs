//! quoted-form-contains-stray-unquote detection.

pub use crate::quoted_form_contains_stray_unquote::domain::{
    QuotedFormContainsStrayUnquoteItem, build_quoted_form_contains_stray_unquote_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, like every other report in this
/// package: the finding is worth surfacing, but it is a build-breaking one only
/// in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<QuotedFormContainsStrayUnquoteItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} stray unquote(s) inside a quoted form",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
