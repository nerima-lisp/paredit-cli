//! a slot with no :initform and no :initarg that a method in the file reads, across explicit files.

pub use crate::defclass_required_slot_no_initform_or_initarg::domain::{
    DefclassRequiredSlotNoInitformOrInitargItem,
    build_defclass_required_slot_no_initform_or_initarg_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: this is a finding a project decides
/// is build-breaking, not one this tool decides for it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DefclassRequiredSlotNoInitformOrInitargItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} unbound-on-read slot(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
