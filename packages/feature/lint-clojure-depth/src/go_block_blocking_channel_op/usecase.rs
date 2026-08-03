//! `go-block-blocking-channel-op` detection across explicit files.

pub use crate::go_block_blocking_channel_op::domain::{
    GoBlockBlockingChannelOpItem, build_go_block_blocking_channel_op_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<GoBlockBlockingChannelOpItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} blocking channel operation(s) on a go-block thread",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
