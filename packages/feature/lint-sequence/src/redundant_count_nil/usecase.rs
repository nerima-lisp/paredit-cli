//! Redundant :count nil ((remove x seq :count nil) is (remove x seq)) detection.

pub use crate::redundant_count_nil::domain::{
    RedundantCountNilItem, RedundantCountNilPolicy, RedundantCountNilPolicyOptions,
    RedundantCountNilSummary, collect_redundant_count_nils, evaluate_redundant_count_nil_policy,
    summarize_redundant_count_nils,
};
