//! Nthcdr-small-index ((nthcdr 1..4 x) is (cdr x)/(cddr x)/...) detection.

pub use crate::nthcdr_small_index::domain::{
    NthcdrSmallIndexItem, build_nthcdr_small_index_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A bare count where a named accessor
/// reads better is a style call, and only a project that has made it can break
/// its own build over it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NthcdrSmallIndexItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} small-count nthcdr call(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
