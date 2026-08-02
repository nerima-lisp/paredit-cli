//! Circular `in-package` chain detection across explicit files.

pub use crate::package_circular_in_package_chain::domain::{
    CircularInPackageChainItem, build_circular_in_package_chain_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A split package region is a
/// readability and load-order hazard, but it is a build-breaking one only in a
/// project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<CircularInPackageChainItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} re-enters {} package(s) it had already left",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
