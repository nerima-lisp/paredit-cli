//! `eq`-on-a-number-literal detection across explicit files.

pub use crate::domain::eq_number_comparison_report::{
    EqNumberComparisonItem, EqNumberComparisonPolicy, EqNumberComparisonPolicyOptions,
    EqNumberComparisonSummary, collect_eq_number_comparisons, evaluate_eq_number_comparison_policy,
    summarize_eq_number_comparisons,
};
