//! Negated-comparison (`(not (= a b))`, better written `(/= a b)`) detection
//! across explicit files.

pub use crate::domain::negated_comparison_report::{
    NegatedComparisonItem, NegatedComparisonPolicy, NegatedComparisonPolicyOptions,
    NegatedComparisonSummary, collect_negated_comparisons, evaluate_negated_comparison_policy,
    summarize_negated_comparisons,
};
