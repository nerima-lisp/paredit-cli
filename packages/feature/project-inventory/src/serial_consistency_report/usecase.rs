//! inspect serial-consistency reporting across a set of files.

pub use crate::serial_consistency_report::domain::{
    ComponentFinding, SerialFault, build_serial_consistency_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every entry is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_fault_policy(
    fail_on_fault: bool,
    reports: &[FileFindings<ComponentFinding>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| report.retained(|finding| finding.fault.is_fault()))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_fault.then_some("--fail-on-fault"),
        &failing,
        |report| {
            format!(
                "{} has {} component(s) inconsistent with their system",
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
