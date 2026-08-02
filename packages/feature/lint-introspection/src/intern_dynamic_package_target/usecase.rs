//! An `intern` whose package argument is computed, across explicit files.

pub use crate::intern_dynamic_package_target::domain::{
    InternDynamicPackageTargetItem, build_intern_dynamic_package_target_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: a computed package target is a
/// finding a project decides is build-breaking, not one this tool decides for
/// it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<InternDynamicPackageTargetItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} intern(s) with a computed package target",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
