//! a defmethod qualifier outside :before, :after and :around, across explicit files.

pub use crate::method_qualifier_typo::domain::{
    MethodQualifierTypoItem, build_method_qualifier_typo_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: this is a finding a project decides
/// is build-breaking, not one this tool decides for it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<MethodQualifierTypoItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} unrecognized method qualifier(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
