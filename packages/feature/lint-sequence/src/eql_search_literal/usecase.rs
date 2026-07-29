//! Default-`eql`-search-for-a-literal (`(member "x" list)`, `(assoc '(a) al)` —
//! the default eql test never matches a string/list literal) detection across
//! explicit files.

pub use crate::eql_search_literal::domain::{
    EqlSearchLiteralItem, build_eql_search_literal_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A search that silently never matches
/// is a defect, but it is a build-breaking one only in a project that has
/// decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<EqlSearchLiteralItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} default-eql literal search(es)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
