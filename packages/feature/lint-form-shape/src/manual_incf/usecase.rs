//! Manual-`incf`/`decf` (`(setf x (1+ x))`, better written `(incf x)`) detection
//! across explicit files.

pub use crate::manual_incf::domain::{ManualIncfItem, build_manual_incf_report};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A hand-written increment is correct
/// code that states its intent indirectly, so it is build-breaking only in a
/// project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<ManualIncfItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} manual increment(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
