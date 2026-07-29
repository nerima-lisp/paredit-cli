//! Redundant getf nil default ((getf p k nil) is (getf p k)) detection.

pub use crate::getf_default_nil::domain::{GetfDefaultNilItem, build_getf_default_nil_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A restated default is noise, but it
/// is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<GetfDefaultNilItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} explicit nil getf default(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
