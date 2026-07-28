//! Nested-boolean (`(or a (or b c))` is `(or a b c)`) detection across explicit
//! files.

pub use crate::nested_boolean::domain::{
    NestedBooleanItem, NestedBooleanPolicy, NestedBooleanPolicyOptions, NestedBooleanSummary,
    collect_nested_booleans, evaluate_nested_boolean_policy, summarize_nested_booleans,
};
