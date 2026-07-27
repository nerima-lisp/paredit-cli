//! Double-reverse ((reverse (reverse x)) is (copy-seq x)) detection.

pub use crate::double_reverse::domain::{
    DoubleReverseItem, DoubleReversePolicy, DoubleReversePolicyOptions, DoubleReverseSummary,
    collect_double_reverses, evaluate_double_reverse_policy, summarize_double_reverses,
};
