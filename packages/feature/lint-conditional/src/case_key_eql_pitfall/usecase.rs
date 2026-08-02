//! Case-key `eql` pitfall detection across explicit files.

pub use crate::case_key_eql_pitfall::domain::{
    CaseKeyEqlPitfallItem, PitfallKind, build_case_key_eql_pitfall_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A string key is very nearly always a
/// defect and a float key depends on reader state, but neither is a build
/// breaker until a project says it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<CaseKeyEqlPitfallItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} case key(s) eql does not match dependably",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
