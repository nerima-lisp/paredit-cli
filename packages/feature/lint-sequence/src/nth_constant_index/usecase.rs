//! Nth-constant-index (`(nth 0 x)`, better written `(first x)`) detection across
//! explicit files.

pub use crate::domain::nth_constant_index_report::{
    NthConstantIndexItem, NthConstantIndexPolicy, NthConstantIndexPolicyOptions,
    NthConstantIndexSummary, collect_nth_constant_indexes, evaluate_nth_constant_index_policy,
    summarize_nth_constant_indexes,
};
