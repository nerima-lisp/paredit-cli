//! Redundant-`the` (`(the t x)` is `x`) detection across explicit files.

pub use crate::domain::redundant_the_report::{
    RedundantTheItem, RedundantThePolicy, RedundantThePolicyOptions, RedundantTheSummary,
    collect_redundant_thes, evaluate_redundant_the_policy, summarize_redundant_thes,
};
