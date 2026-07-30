//! Coerce-to-t ((coerce x t) is x) detection.

pub use crate::coerce_to_t::domain::{CoerceToTItem, build_coerce_to_t_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A no-op coercion is noise, but it is
/// a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<CoerceToTItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} no-op coerce(s) to t",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
