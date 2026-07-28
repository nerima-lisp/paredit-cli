//! Redundant `:from-end nil` (`(find x seq :from-end nil)` is `(find x seq)`)
//! detection across explicit files.

pub use crate::redundant_from_end_nil::domain::{
    RedundantFromEndNilItem, RedundantFromEndNilPolicy, RedundantFromEndNilPolicyOptions,
    RedundantFromEndNilSummary, collect_redundant_from_end_nils,
    evaluate_redundant_from_end_nil_policy, summarize_redundant_from_end_nils,
};
