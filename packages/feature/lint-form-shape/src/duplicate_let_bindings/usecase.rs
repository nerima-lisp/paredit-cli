//! Duplicate parallel-`let` binding detection across explicit files.

pub use crate::duplicate_let_bindings::domain::{
    DuplicateLetBindingItem, build_duplicate_let_binding_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A parallel `let` that binds a name
/// twice has undefined consequences, but whether that stops a build is the
/// project's call.
#[must_use]
pub fn evaluate_fail_on_duplicate_policy(
    fail_on_duplicate: bool,
    reports: &[FileFindings<DuplicateLetBindingItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_duplicate.then_some("--fail-on-duplicate"),
        reports,
        |report| {
            format!(
                "{} has {} duplicated let binding(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
