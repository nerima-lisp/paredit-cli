//! Negated-comparison (`(not (= a b))`, better written `(/= a b)`) detection
//! across explicit files.

pub use crate::negated_comparison::domain::{
    NegatedComparisonItem, NegatedComparisonPolicy, NegatedComparisonPolicyOptions,
    NegatedComparisonSummary, collect_negated_comparisons, evaluate_negated_comparison_policy,
    summarize_negated_comparisons,
};
