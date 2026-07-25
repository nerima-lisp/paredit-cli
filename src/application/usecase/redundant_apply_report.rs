//! Redundant-`apply` (`(apply #'f (list a b))`, which is just `(f a b)`)
//! detection across explicit files.

pub use crate::domain::redundant_apply_report::{
    RedundantApplyItem, RedundantApplyPolicy, RedundantApplyPolicyOptions, RedundantApplySummary,
    collect_redundant_applies, evaluate_redundant_apply_policy, summarize_redundant_applies,
};
