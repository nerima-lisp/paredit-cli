//! Nthcdr-zero (`(nthcdr 0 list)` is `list`) detection across explicit files.

pub use crate::nthcdr_zero::domain::{NthcdrZeroItem, build_nthcdr_zero_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A no-op `nthcdr` is dead weight, but
/// it is a build-breaking one only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NthcdrZeroItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} zero-count nthcdr call(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
