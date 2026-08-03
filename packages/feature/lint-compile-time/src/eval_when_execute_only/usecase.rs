//! Compile-file-invisible `eval-when` detection across explicit files.

pub use crate::eval_when_execute_only::domain::{
    EvalWhenExecuteOnlyItem, build_eval_when_execute_only_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: a file that is only ever `load`ed as
/// source — a script, a `--load` snippet — is not wrong to confine a definition
/// to `:execute`.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<EvalWhenExecuteOnlyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has an eval-when whose body compile-file discards",
                report.path.display()
            )
        },
    )
}
