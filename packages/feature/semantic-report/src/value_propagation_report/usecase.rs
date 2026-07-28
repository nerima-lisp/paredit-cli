//! Constant-propagation coverage reporting across a set of files.

pub use crate::value_propagation_report::domain::{
    BlockedBinding, BlockedReason, PropagatedBinding, ValuePropagationReportFile,
    build_value_propagation_report,
};

#[derive(Debug, Clone, Copy)]
pub struct ValuePropagationPolicyOptions {
    /// The lowest acceptable resolved/seen ratio, or `None` to assert nothing.
    pub min_coverage: Option<f64>,
}

impl ValuePropagationPolicyOptions {
    #[must_use]
    pub const fn new(min_coverage: Option<f64>) -> Self {
        Self { min_coverage }
    }
}

#[derive(Debug, Clone)]
pub struct ValuePropagationPolicy {
    pub min_coverage: Option<f64>,
    pub propagated_count: usize,
    pub blocked_count: usize,
    pub coverage: f64,
    /// How many bindings each reason accounts for, in the order the reasons
    /// are checked. Present even at zero: an omitted key reads as "not
    /// measured", and every reason is always measured.
    pub blocked_by_reason: Vec<(BlockedReason, usize)>,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[must_use]
pub fn evaluate_value_propagation_policy(
    options: ValuePropagationPolicyOptions,
    reports: &[ValuePropagationReportFile],
) -> ValuePropagationPolicy {
    let propagated_count: usize = reports.iter().map(|report| report.propagated.len()).sum();
    let blocked_count: usize = reports.iter().map(|report| report.blocked.len()).sum();
    let total = propagated_count + blocked_count;
    let coverage = if total == 0 {
        1.0
    } else {
        propagated_count as f64 / total as f64
    };

    let blocked_by_reason = BlockedReason::ALL
        .into_iter()
        .map(|reason| {
            (
                reason,
                reports
                    .iter()
                    .map(|report| report.blocked_by(reason))
                    .sum::<usize>(),
            )
        })
        .collect();

    let violations = options
        .min_coverage
        .filter(|min| coverage < *min)
        .map(|min| format!("propagation coverage {coverage:.3} is below the required {min:.3}"))
        .into_iter()
        .collect::<Vec<_>>();

    ValuePropagationPolicy {
        min_coverage: options.min_coverage,
        propagated_count,
        blocked_count,
        coverage,
        blocked_by_reason,
        passed: violations.is_empty(),
        violations,
    }
}
