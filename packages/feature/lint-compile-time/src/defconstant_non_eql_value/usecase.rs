//! Non-`eql` constant detection across explicit files.

pub use crate::defconstant_non_eql_value::domain::{
    DefconstantNonEqlValueItem, build_defconstant_non_eql_value_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<DefconstantNonEqlValueItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} defines a constant whose value is not eql to itself",
                report.path.display()
            )
        },
    )
}
