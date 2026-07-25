//! Single-argument numeric comparison (`(< x)`, `(= x)`, … — vacuously true)
//! detection across explicit files.

pub use crate::domain::single_arg_comparison_report::{
    SingleArgComparisonItem, SingleArgComparisonPolicy, SingleArgComparisonPolicyOptions,
    SingleArgComparisonSummary, collect_single_arg_comparisons,
    evaluate_single_arg_comparison_policy, summarize_single_arg_comparisons,
};
