//! T-comparison (`(eq X t)` and friends, a generalized-boolean smell) detection
//! across explicit files.

pub use crate::t_comparison::domain::{
    TComparisonItem, TComparisonPolicy, TComparisonPolicyOptions, TComparisonSummary,
    collect_t_comparisons, evaluate_t_comparison_policy, summarize_t_comparisons,
};
