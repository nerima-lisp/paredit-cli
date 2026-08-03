//! Unspecified `memq`/`assq` search detection across explicit files.

pub use crate::memq_assq_literal_key::domain::{
    MemqAssqLiteralKeyItem, build_memq_assq_literal_key_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<MemqAssqLiteralKeyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} unspecified memq/assq search(es)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
