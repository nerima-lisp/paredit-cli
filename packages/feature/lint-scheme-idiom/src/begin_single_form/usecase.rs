//! Single-form `begin` detection across explicit files.

pub use crate::begin_single_form::domain::{BeginSingleFormItem, build_begin_single_form_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: a redundant `begin` is noise, but it
/// is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<BeginSingleFormItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} single-form begin(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
