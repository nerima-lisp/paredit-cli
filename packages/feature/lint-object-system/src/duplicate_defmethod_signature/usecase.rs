//! two defmethods with the same name, qualifiers and specializers, across explicit files.

pub use crate::duplicate_defmethod_signature::domain::{
    DuplicateDefmethodSignatureItem, build_duplicate_defmethod_signature_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: this is a finding a project decides
/// is build-breaking, not one this tool decides for it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DuplicateDefmethodSignatureItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} duplicate defmethod signature(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
