//! Nested-`unless` (`(unless a (unless b body))` is `(unless (or a b) body)`)
//! detection across explicit files.

pub use crate::domain::nested_unless_report::{
    NestedUnlessItem, NestedUnlessPolicy, NestedUnlessPolicyOptions, NestedUnlessSummary,
    collect_nested_unlesses, evaluate_nested_unless_policy, summarize_nested_unlesses,
};
