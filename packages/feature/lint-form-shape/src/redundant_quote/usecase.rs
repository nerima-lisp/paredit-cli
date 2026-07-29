//! Redundant-quote-of-a-self-evaluating-literal detection across explicit files.

pub use crate::redundant_quote::domain::{RedundantQuoteItem, build_redundant_quote_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A redundant quote is noise, but it is
/// a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<RedundantQuoteItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant quote(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
