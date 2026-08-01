//! a print-object method that writes to the stream directly, across explicit files.

pub use crate::print_object_without_print_unreadable_object::domain::{
    PrintObjectWithoutPrintUnreadableObjectItem,
    build_print_object_without_print_unreadable_object_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: this is a finding a project decides
/// is build-breaking, not one this tool decides for it.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<PrintObjectWithoutPrintUnreadableObjectItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} unwrapped print-object method(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
