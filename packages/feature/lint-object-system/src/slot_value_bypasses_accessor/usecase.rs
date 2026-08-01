//! a slot-value read of a slot the file declares an accessor for, across explicit files.

pub use crate::slot_value_bypasses_accessor::domain::{
    SlotValueBypassesAccessorItem, build_slot_value_bypasses_accessor_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: this is a finding a project decides
/// is build-breaking, not one this tool decides for it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SlotValueBypassesAccessorItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} accessor-bypassing slot-value read(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
