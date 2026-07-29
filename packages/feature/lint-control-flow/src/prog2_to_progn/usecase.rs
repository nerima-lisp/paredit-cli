//! Prog2-to-progn ((prog2 a b) is (progn a b)) detection.

pub use crate::prog2_to_progn::domain::{Prog2ToPrognItem, build_prog2_to_progn_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A two-form `prog2` returns exactly
/// what the equivalent `progn` returns, so it is build-breaking only in a
/// project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<Prog2ToPrognItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} two-form prog2 form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
