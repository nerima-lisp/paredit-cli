//! Redundant-divisor ((floor x 1) is (floor x)) detection.

pub use crate::domain::redundant_divisor_report::{
    RedundantDivisorItem, RedundantDivisorPolicy, RedundantDivisorPolicyOptions,
    RedundantDivisorSummary, collect_redundant_divisors, evaluate_redundant_divisor_policy,
    summarize_redundant_divisors,
};
