//! Redundant-`progn` (empty, or wrapping a single form) detection across
//! explicit files.

pub use crate::domain::redundant_progn_report::{
    RedundantPrognItem, RedundantPrognPolicy, RedundantPrognPolicyOptions, RedundantPrognSummary,
    collect_redundant_progns, evaluate_redundant_progn_policy, summarize_redundant_progns,
};
