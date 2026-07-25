//! Self-comparison (`(eq x x)`) detection across explicit files.

pub use crate::domain::self_comparison_report::{
    SelfComparisonItem, SelfComparisonPolicy, SelfComparisonPolicyOptions, SelfComparisonSummary,
    collect_self_comparisons, evaluate_self_comparison_policy, summarize_self_comparisons,
};
