//! inspect loop reporting across a set of files.

pub use crate::loop_report::domain::{LoopForm, build_loop_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every finding is listed, but only the defective ones can fail a build.
#[must_use]
pub fn evaluate_fail_on_unterminated_policy(
    fail_on_unterminated: bool,
    reports: &[FileFindings<LoopForm>],
) -> ReportPolicy {
    let failing = reports
        .iter()
        .map(|report| report.retained(|form| !form.terminates))
        .collect::<Vec<_>>();

    let mut policy = ReportPolicy::fail_on_any(
        fail_on_unterminated.then_some("--fail-on-unterminated"),
        &failing,
        |report| {
            format!(
                "{} has {} loop(s) with no termination clause",
                report.path.display(),
                report.findings.len()
            )
        },
    );
    // The headline count stays the number of findings reported; only the
    // gate narrows.
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
