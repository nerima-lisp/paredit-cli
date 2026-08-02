//! Nested-definition parameter shadowing across explicit files.

pub use crate::nested_function_parameter_shadows_enclosing_parameter::domain::{
    NestedParameterShadowItem, build_nested_parameter_shadow_report,
};

use paredit_core_cli::report::{FileFindings, ReportPolicy};

/// Evaluates this report's gate.
///
/// Armed by a flag rather than always on. A shadowed parameter compiles and
/// runs; it is a build-breaking defect only in a project that has decided it is.
#[must_use]
pub fn evaluate_fail_on_violation_policy(
    fail_on_violation: bool,
    reports: &[FileFindings<NestedParameterShadowItem>],
) -> ReportPolicy {
    ReportPolicy::fail_on_any(
        fail_on_violation.then_some("--fail-on-violation"),
        reports,
        |report| {
            format!(
                "{} has {} shadowed nested parameter(s)",
                report.path.display(),
                report.findings.len()
            )
        },
    )
}
