//! `atom-swap-with-side-effect` detection across explicit files.

pub use crate::atom_swap_with_side_effect::domain::{
    AtomSwapWithSideEffectItem, build_atom_swap_with_side_effect_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<AtomSwapWithSideEffectItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} swap! update function(s) that perform a side effect",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
