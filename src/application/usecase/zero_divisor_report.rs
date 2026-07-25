//! Zero-divisor ((/ x 0) is a division by zero) detection.

pub use crate::domain::zero_divisor_report::{
    ZeroDivisorItem, ZeroDivisorPolicy, ZeroDivisorPolicyOptions, ZeroDivisorSummary,
    collect_zero_divisors, evaluate_zero_divisor_policy, summarize_zero_divisors,
};
