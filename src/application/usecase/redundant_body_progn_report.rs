//! Redundant-body-`progn` (a multi-form progn in an implicit-progn macro body,
//! e.g. `(when c (progn a b))`) detection across explicit files.

pub use crate::domain::redundant_body_progn_report::{
    RedundantBodyPrognItem, RedundantBodyPrognPolicy, RedundantBodyPrognPolicyOptions,
    RedundantBodyPrognSummary, collect_redundant_body_progns, evaluate_redundant_body_progn_policy,
    summarize_redundant_body_progns,
};
