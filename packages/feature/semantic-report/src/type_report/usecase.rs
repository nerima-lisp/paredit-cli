//! Proved-type reporting across a set of files.

pub use crate::type_report::domain::{
    TypeReportFile, TypedBinding, TypedExpression, build_type_report,
};

/// Whether a run should fail because a declaration contradicted inference.
#[derive(Debug, Clone, Copy)]
pub struct TypeReportPolicyOptions {
    pub fail_on_contradiction: bool,
}

impl TypeReportPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_contradiction: bool) -> Self {
        Self {
            fail_on_contradiction,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TypeReportPolicy {
    pub fail_on_contradiction: bool,
    pub typed_binding_count: usize,
    pub typed_expression_count: usize,
    pub contradiction_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[must_use]
pub fn evaluate_type_report_policy(
    options: TypeReportPolicyOptions,
    reports: &[TypeReportFile],
) -> TypeReportPolicy {
    let typed_binding_count = reports.iter().map(|report| report.bindings.len()).sum();
    let typed_expression_count = reports.iter().map(|report| report.expressions.len()).sum();
    let contradiction_count = reports.iter().map(TypeReportFile::contradictions).sum();

    let violations = reports
        .iter()
        .filter(|report| options.fail_on_contradiction && report.contradictions() > 0)
        .map(|report| {
            format!(
                "{} declares {} type(s) no object can satisfy",
                report.path.display(),
                report.contradictions()
            )
        })
        .collect::<Vec<_>>();

    TypeReportPolicy {
        fail_on_contradiction: options.fail_on_contradiction,
        typed_binding_count,
        typed_expression_count,
        contradiction_count,
        passed: violations.is_empty(),
        violations,
    }
}
