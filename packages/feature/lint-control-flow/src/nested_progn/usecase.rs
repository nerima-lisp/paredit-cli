//! Nested-`progn` (a multi-form progn spliced into another progn) detection
//! across explicit files.

pub use crate::nested_progn::domain::{
    NestedPrognItem, NestedPrognPolicy, NestedPrognPolicyOptions, NestedPrognSummary,
    collect_nested_progns, evaluate_nested_progn_policy, summarize_nested_progns,
};
