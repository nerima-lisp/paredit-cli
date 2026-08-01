//! inspect magic-numbers reporting across a set of files.

pub use crate::magic_number_report::domain::{MagicNumberItem, build_magic_number_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, and narrower than the report:
/// every literal is listed, but only a build that opts in can fail on them.
#[must_use]
pub fn evaluate_fail_on_magic_number_policy(
    fail_on_magic_number: bool,
    reports: &[FileFindings<MagicNumberItem>],
) -> ReportPolicy {
    let mut policy = ReportPolicy::fail_on_any(
        fail_on_magic_number.then_some("--fail-on-magic-number"),
        reports,
        |report| {
            format!(
                "{} has {} magic number(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    );
    // The headline count stays the number of entries reported; only
    // the gate narrows.
    policy.finding_count = reports.iter().map(|report| report.findings.len()).sum();
    policy
}
