//! Missing-`format`-destination (`(format "…" …)` — a string literal where the
//! destination belongs) detection across explicit files.

pub use crate::format_missing_destination::domain::{
    FormatMissingDestinationItem, build_format_missing_destination_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A string literal in the destination
/// slot is never what the author meant, but it is a build-breaking defect only
/// in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<FormatMissingDestinationItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} format call(s) missing a destination",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
