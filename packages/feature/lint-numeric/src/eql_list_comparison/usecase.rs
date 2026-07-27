//! `eq`/`eql`-on-a-quoted-list detection across explicit files.

pub use crate::eql_list_comparison::domain::{
    EqlListComparisonItem, EqlListComparisonPolicy, EqlListComparisonPolicyOptions,
    EqlListComparisonSummary, collect_eql_list_comparisons, evaluate_eql_list_comparison_policy,
    summarize_eql_list_comparisons,
};
