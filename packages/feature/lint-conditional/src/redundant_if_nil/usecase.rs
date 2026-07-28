//! Redundant-`if`-`nil`-else (`(if test then nil)` — the explicit nil else is
//! redundant) detection across explicit files.

pub use crate::redundant_if_nil::domain::{
    RedundantIfNilItem, RedundantIfNilPolicy, RedundantIfNilPolicyOptions, RedundantIfNilSummary,
    collect_redundant_if_nils, evaluate_redundant_if_nil_policy, summarize_redundant_if_nils,
};
