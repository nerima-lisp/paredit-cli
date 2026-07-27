//! Redundant-`let*` (`let*` with ≤ 1 binding is `let`) detection across
//! explicit files.

pub use crate::domain::redundant_let_star_report::{
    RedundantLetStarItem, RedundantLetStarPolicy, RedundantLetStarPolicyOptions,
    RedundantLetStarSummary, collect_redundant_let_stars, evaluate_redundant_let_star_policy,
    summarize_redundant_let_stars,
};
