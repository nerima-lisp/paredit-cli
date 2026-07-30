//! inspect class-hierarchy reporting across a set of files.

pub use crate::class_hierarchy_report::domain::{ClassFinding, build_class_hierarchy_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report: every
/// finding is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_shadowed_slot_policy(
    fail_on_shadowed_slot: bool,
    reports: &[FileFindings<ClassFinding>],
) -> ReportPolicy {
    // The gate fires on a subset of the findings, not on any finding at all:
    // a class shadowing an inherited slot is the finding; every class is listed.
    let failing = reports
        .iter()
        .map(|report| report.retained(|class| !class.shadowed_slots.is_empty()))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_shadowed_slot.then_some("--fail-on-shadowed-slot"),
        &failing,
        |report| {
            format!(
                "{} has {} class(es) shadowing an inherited slot",
                report.path.display(),
                report.findings.len()
            )
        },
    );
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
