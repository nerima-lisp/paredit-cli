//! Declared-but-never-entered package detection across explicit files.

pub use crate::defpackage_without_in_package::domain::{
    DefpackageWithoutInPackageItem, build_defpackage_without_in_package_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: a project may deliberately keep a
/// declaration in a file the loader enters some other way.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DefpackageWithoutInPackageItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} declares a package it never enters",
                report.path.display()
            )
        },
    )
}
