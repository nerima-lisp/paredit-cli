//! Constant-folding reporting across a set of files.

pub use crate::constant_report::domain::{
    ConstantReportFile, FileConstant, FoldableExpression, build_constant_report,
};

#[derive(Debug, Clone, Copy)]
pub struct ConstantReportPolicyOptions {
    pub fail_on_foldable: bool,
}

impl ConstantReportPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_foldable: bool) -> Self {
        Self { fail_on_foldable }
    }
}

#[derive(Debug, Clone)]
pub struct ConstantReportPolicy {
    pub fail_on_foldable: bool,
    pub foldable_count: usize,
    pub constant_count: usize,
    /// Bytes the whole set of folds would remove. Signed: a fold to a long
    /// string can cost width, and hiding that behind a saturating subtraction
    /// would make the total a claim rather than a measurement.
    pub saved_bytes: i64,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[must_use]
pub fn evaluate_constant_report_policy(
    options: ConstantReportPolicyOptions,
    reports: &[ConstantReportFile],
) -> ConstantReportPolicy {
    let foldable_count = reports.iter().map(|report| report.foldable.len()).sum();
    let constant_count = reports.iter().map(|report| report.constants.len()).sum();
    let saved_bytes = reports
        .iter()
        .flat_map(|report| report.foldable.iter().map(|item| item.saved_bytes))
        .sum();

    let violations = reports
        .iter()
        .filter(|report| options.fail_on_foldable && !report.foldable.is_empty())
        .map(|report| {
            format!(
                "{} has {} expression(s) that need not be computed at run time",
                report.path.display(),
                report.foldable.len()
            )
        })
        .collect::<Vec<_>>();

    ConstantReportPolicy {
        fail_on_foldable: options.fail_on_foldable,
        foldable_count,
        constant_count,
        saved_bytes,
        passed: violations.is_empty(),
        violations,
    }
}
