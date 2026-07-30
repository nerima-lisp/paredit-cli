//! inspect api-surface reporting across a set of files.

pub use crate::api_surface_report::domain::{ApiEntry, build_api_surface_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every entry is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_undefined_export_policy(
    fail_on_undefined_export: bool,
    reports: &[FileFindings<ApiEntry>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| report.retained(|entry| !entry.defined))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_undefined_export.then_some("--fail-on-undefined-export"),
        &failing,
        |report| {
            format!(
                "{} exports {} symbol(s) nothing defines",
                report.path.display(),
                report.findings.len()
            )
        },
    );
    // The headline count stays the number of entries reported; only
    // the gate narrows.
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
