//! Cons-to-`list` (`(cons a nil)` is `(list a)`, `(cons a (list b))` is
//! `(list a b)`) detection across explicit files.

pub use crate::cons_to_list::domain::{ConsToListItem, collect_cons_to_lists};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A `cons` onto `nil` or a list
/// literal is a `list` written the long way, not a wrong program, so only a
/// project that has decided it is may break its build on one.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ConsToListItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} collapsible cons(es)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
