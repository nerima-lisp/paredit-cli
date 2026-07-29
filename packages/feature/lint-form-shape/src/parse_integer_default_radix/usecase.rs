//! Redundant :radix 10 ((parse-integer s :radix 10) is (parse-integer s)) detection.

pub use crate::parse_integer_default_radix::domain::{
    ParseIntegerDefaultRadixItem, build_parse_integer_default_radix_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A restated default is noise, but it
/// is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ParseIntegerDefaultRadixItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} explicit :radix 10 argument(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
