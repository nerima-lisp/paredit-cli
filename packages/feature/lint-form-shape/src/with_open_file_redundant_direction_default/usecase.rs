//! with-open-file-redundant-direction-default detection.

pub use crate::with_open_file_redundant_direction_default::domain::{
    WithOpenFileRedundantDirectionDefaultItem,
    build_with_open_file_redundant_direction_default_report,
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
    reports: &[FileFindings<WithOpenFileRedundantDirectionDefaultItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} redundant :direction :input argument(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
