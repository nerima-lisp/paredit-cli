//! Function definitions installed under a run-time-built name, across explicit
//! files.

pub use crate::symbol_function_fset_dynamic_name::domain::{
    SymbolFunctionFsetDynamicNameItem, build_symbol_function_fset_dynamic_name_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: a project that generates its API on
/// purpose has decided this is how it works.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<SymbolFunctionFsetDynamicNameItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} installs {} function definition(s) under a run-time-built name",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
