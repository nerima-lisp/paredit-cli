//! A nested `block` reusing an enclosing block's name, across explicit files.

pub use crate::block_name_shadows_outer_block::domain::{
    BlockNameShadowsOuterBlockItem, build_block_name_shadows_outer_block_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, like every other report in this
/// package: what this rule reports is a defect, but a build-breaking one only
/// in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<BlockNameShadowsOuterBlockItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} shadowed block name(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
