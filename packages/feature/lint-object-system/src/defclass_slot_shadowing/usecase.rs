//! a subclass slot that silently shadows a same-file superclass slot, across explicit files.

pub use crate::defclass_slot_shadowing::domain::{
    DefclassSlotShadowingItem, build_defclass_slot_shadowing_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: this is a finding a project decides
/// is build-breaking, not one this tool decides for it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DefclassSlotShadowingItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} shadowed slot(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
