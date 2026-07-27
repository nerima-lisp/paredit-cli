//! Nthcdr-small-index ((nthcdr 1..4 x) is (cdr x)/(cddr x)/...) detection.

pub use crate::nthcdr_small_index::domain::{
    NthcdrSmallIndexItem, NthcdrSmallIndexPolicy, NthcdrSmallIndexPolicyOptions,
    NthcdrSmallIndexSummary, collect_nthcdr_small_indexes, evaluate_nthcdr_small_index_policy,
    summarize_nthcdr_small_indexes,
};
