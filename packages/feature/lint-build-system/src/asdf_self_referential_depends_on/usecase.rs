//! Self-referential-dependency detection across explicit files.

pub use crate::asdf_self_referential_depends_on::domain::{
    AsdfSelfReferentialDependsOnItem, build_asdf_self_referential_depends_on_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, for consistency with every other
/// report's gate — even though, unlike most, a finding here is a build ASDF
/// refuses to perform.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<AsdfSelfReferentialDependsOnItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} self-referential dependency(ies)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
