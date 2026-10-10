pub use super::domain::{TestRiskReport, build_test_risk_report};
use paredit_core_cli::report::ReportPolicy;

#[must_use]
pub fn evaluate_test_risk_policy(enabled: bool, report: &TestRiskReport) -> ReportPolicy {
    let count = report.ranked.len();
    ReportPolicy {
        gate: enabled.then_some("--fail-on-untested"),
        finding_count: count,
        passed: !enabled || count == 0,
        violations: if enabled && count > 0 {
            vec![format!(
                "{count} modelled function(s) have no naming-convention test link"
            )]
        } else {
            Vec::new()
        },
    }
}
