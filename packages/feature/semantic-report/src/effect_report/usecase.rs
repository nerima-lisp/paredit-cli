//! Side-effect classification reporting across a set of files.

pub use crate::effect_report::domain::{
    DefinitionEffect, EffectReportFile, Purity, build_effect_report,
};

#[derive(Debug, Clone, Copy)]
pub struct EffectPolicyOptions {
    /// Fail when any definition's purity cannot be decided. The gate a caller
    /// reaches for when they intend the file to be provably analyzable.
    pub fail_on_unknown: bool,
}

impl EffectPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_unknown: bool) -> Self {
        Self { fail_on_unknown }
    }
}

#[derive(Debug, Clone)]
pub struct EffectPolicy {
    pub fail_on_unknown: bool,
    pub definition_count: usize,
    /// One count per verdict, always all three. A missing key would read as
    /// "not measured", and every definition is always classified.
    pub by_purity: Vec<(Purity, usize)>,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[must_use]
pub fn evaluate_effect_policy(
    options: EffectPolicyOptions,
    reports: &[EffectReportFile],
) -> EffectPolicy {
    let definition_count = reports.iter().map(|report| report.definitions.len()).sum();
    let by_purity = Purity::ALL
        .into_iter()
        .map(|purity| {
            (
                purity,
                reports
                    .iter()
                    .map(|report| report.count_of(purity))
                    .sum::<usize>(),
            )
        })
        .collect();

    let violations = reports
        .iter()
        .filter(|report| options.fail_on_unknown && report.count_of(Purity::Unknown) > 0)
        .map(|report| {
            format!(
                "{} has {} definition(s) whose effects cannot be decided",
                report.path.display(),
                report.count_of(Purity::Unknown)
            )
        })
        .collect::<Vec<_>>();

    EffectPolicy {
        fail_on_unknown: options.fail_on_unknown,
        definition_count,
        by_purity,
        passed: violations.is_empty(),
        violations,
    }
}
