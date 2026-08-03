//! Unbalanced-bracketing-construct (`~[` `~{` `~<` `~(`) detection.

pub use crate::format_nested_directive_unbalanced::domain::{
    FormatNestedDirectiveUnbalancedItem, build_format_nested_directive_unbalanced_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on, in line with every other report
/// in this package — even though CLHS 22.3.10.1 requires these constructs
/// to nest and an implementation is entitled to refuse the string outright.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<FormatNestedDirectiveUnbalancedItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} control string(s) whose bracketing constructs do not nest",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
