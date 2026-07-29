//! Modify-macro-arity (an incf/decf/push/pop with the wrong argument count)
//! detection across explicit files.

pub use crate::modify_macro_arity::domain::{
    ModifyMacroArityItem, build_modify_macro_arity_report, expected_arity_phrase,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A wrong argument count fails at
/// macroexpansion, but it is a build-breaking finding only in a project that
/// has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ModifyMacroArityItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} misarity modify-macro call(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
