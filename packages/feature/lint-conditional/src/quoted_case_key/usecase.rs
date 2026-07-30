//! Quoted-`case`-key (a case/ecase/ccase clause with a quoted key) detection
//! across explicit files.

pub use crate::quoted_case_key::domain::{QuotedCaseKeyItem, build_quoted_case_key_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A quoted `case` key is almost always
/// a bug, but it is a build-breaking one only in a project that has decided it
/// is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<QuotedCaseKeyItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} quoted case key(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
