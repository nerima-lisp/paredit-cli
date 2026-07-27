//! `eq`/`eql`-on-a-string-literal detection across explicit files.

pub use crate::eql_string_comparison::domain::{
    EqlStringComparisonItem, EqlStringComparisonPolicy, EqlStringComparisonPolicyOptions,
    EqlStringComparisonSummary, collect_eql_string_comparisons,
    evaluate_eql_string_comparison_policy, summarize_eql_string_comparisons,
};
