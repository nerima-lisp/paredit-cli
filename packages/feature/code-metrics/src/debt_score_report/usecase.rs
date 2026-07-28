//! inspect debt-score reporting across a set of files.

pub use crate::debt_score_report::domain::{DebtFinding, build_debt_score_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A debt measurement is a fact about the file,
/// not a defect by definition — it is a failure only in a project that has
/// decided it is one.
#[must_use]
pub fn evaluate_fail_on_debt_policy(
    fail_on_debt: bool,
    reports: &[FileFindings<DebtFinding>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_debt.then_some("--fail-on-debt"),
        reports,
        |report| {
            format!(
                "{} carries debt in {} definition(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
