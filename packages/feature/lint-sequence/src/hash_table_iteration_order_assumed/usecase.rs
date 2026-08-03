//! Hash-table iteration-order assumption detection.

pub use crate::hash_table_iteration_order_assumed::domain::{
    HashOrderItem, collect_hash_order_assumptions,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. Reading one element out of a hash
/// table's iteration is only wrong if which element it is matters, and this
/// report cannot know that, so only a project that has decided it always
/// matters may break its build on one.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<HashOrderItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} positional read(s) of a hash table's iteration",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
