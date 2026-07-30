//! Handler-case-no-clauses ((handler-case x) is x) detection.

pub use crate::handler_case_no_clauses::domain::{
    HandlerCaseNoClausesItem, build_handler_case_no_clauses_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A clauseless `handler-case` is a
/// leftover wrapper that changes nothing about what the code does, so it is
/// build-breaking only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<HandlerCaseNoClausesItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} clauseless handler-case form(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
