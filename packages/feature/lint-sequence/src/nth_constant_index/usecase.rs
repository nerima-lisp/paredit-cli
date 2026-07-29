//! Nth-constant-index (`(nth 0 x)`, better written `(first x)`) detection across
//! explicit files.

pub use crate::nth_constant_index::domain::{
    NthConstantIndexItem, build_nth_constant_index_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A bare index where an ordinal reads
/// better is a style call, and only a project that has made it can break its
/// own build over it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NthConstantIndexItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} constant-index nth call(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
