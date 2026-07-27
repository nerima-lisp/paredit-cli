//! Redundant `:from-end nil` (`(find x seq :from-end nil)` is `(find x seq)`)
//! detection across explicit files.

pub use crate::domain::redundant_from_end_nil_report::{
    RedundantFromEndNilItem, RedundantFromEndNilPolicy, RedundantFromEndNilPolicyOptions,
    RedundantFromEndNilSummary, collect_redundant_from_end_nils,
    evaluate_redundant_from_end_nil_policy, summarize_redundant_from_end_nils,
};
