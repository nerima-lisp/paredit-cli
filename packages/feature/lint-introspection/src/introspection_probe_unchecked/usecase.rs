//! Nil-returning introspection probes applied without a check, across explicit
//! files.

pub use crate::introspection_probe_unchecked::domain::{
    IntrospectionProbeUncheckedItem, build_introspection_probe_unchecked_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on: whether an unchecked lookup is
/// build-breaking is a project's decision, not this tool's.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<IntrospectionProbeUncheckedItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} applies {} unchecked introspection probe(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
