//! Redundant last count of 1 ((last x 1) is (last x)) detection.

pub use crate::last_default_count::domain::{
    LastDefaultCountItem, LastDefaultCountPolicy, LastDefaultCountPolicyOptions,
    LastDefaultCountSummary, collect_last_default_counts, evaluate_last_default_count_policy,
    summarize_last_default_counts,
};
