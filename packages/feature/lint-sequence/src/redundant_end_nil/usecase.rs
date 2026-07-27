//! Redundant `:end nil` (`(find x seq :end nil)` is `(find x seq)`) detection
//! across explicit files.

pub use crate::domain::redundant_end_nil_report::{
    RedundantEndNilItem, RedundantEndNilPolicy, RedundantEndNilPolicyOptions,
    RedundantEndNilSummary, collect_redundant_end_nils, evaluate_redundant_end_nil_policy,
    summarize_redundant_end_nils,
};
