//! Nested-`progn` (a multi-form progn spliced into another progn) detection
//! across explicit files.

pub use crate::domain::nested_progn_report::{
    NestedPrognItem, NestedPrognPolicy, NestedPrognPolicyOptions, NestedPrognSummary,
    collect_nested_progns, evaluate_nested_progn_policy, summarize_nested_progns,
};
