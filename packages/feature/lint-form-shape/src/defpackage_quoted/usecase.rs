//! Defpackage-quoted ((:export 'foo) in defpackage is a bug) detection.

pub use crate::defpackage_quoted::domain::{DefpackageQuotedItem, build_defpackage_quoted_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A quoted designator names the wrong
/// symbol, but whether that stops a build is the project's call.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DefpackageQuotedItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} quoted defpackage designator(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
