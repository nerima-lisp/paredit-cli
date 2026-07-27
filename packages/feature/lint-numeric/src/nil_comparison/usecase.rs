//! Nil-comparison (`(eq X nil)` and friends, which are just `(null X)`)
//! detection across explicit files.

pub use crate::domain::nil_comparison_report::{
    NilComparisonItem, NilComparisonPolicy, NilComparisonPolicyOptions, NilComparisonSummary,
    collect_nil_comparisons, evaluate_nil_comparison_policy, summarize_nil_comparisons,
};
