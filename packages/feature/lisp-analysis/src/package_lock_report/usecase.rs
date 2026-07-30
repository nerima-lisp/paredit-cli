//! inspect package-locks reporting across a set of files.

pub use crate::package_lock_report::domain::{PackageLock, build_package_lock_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every finding is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_undefined_behavior_policy(
    fail_on_undefined_behavior: bool,
    reports: &[FileFindings<PackageLock>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| report.retained(|finding| finding.collision.is_undefined_behavior()))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_undefined_behavior.then_some("--fail-on-undefined-behavior"),
        &failing,
        |report| {
            format!(
                "{} redefines or binds {} COMMON-LISP symbol(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    );
    // The headline count stays the number of findings reported; only the
    // gate narrows.
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
