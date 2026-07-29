//! Verbose-negation (`(- 0 x)`, `(* x -1)` are `(- x)`) detection across
//! explicit files.

pub use crate::verbose_negation::domain::{VerboseNegationItem, build_verbose_negation_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Long-hand negation computes the right
/// answer; it is a readability finding, and only a project that has decided
/// readability breaks its build wants an exit code for one.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<VerboseNegationItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} verbose negation(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
