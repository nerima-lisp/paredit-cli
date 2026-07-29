//! inspect macro-hygiene reporting across a set of files.

pub use crate::macro_hygiene_report::domain::{HygieneFinding, build_macro_hygiene_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A hygiene risk is a fact about the file,
/// not a defect by definition — it is a failure only in a project that has
/// decided it is one.
#[must_use]
pub fn evaluate_fail_on_risk_policy(
    fail_on_risk: bool,
    reports: &[FileFindings<HygieneFinding>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_risk.then_some("--fail-on-risk"),
        reports,
        |report| {
            format!(
                "{} has {} macro hygiene risk(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
