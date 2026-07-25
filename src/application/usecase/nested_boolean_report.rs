//! Nested-boolean (`(or a (or b c))` is `(or a b c)`) detection across explicit
//! files.

pub use crate::domain::nested_boolean_report::{
    NestedBooleanItem, NestedBooleanPolicy, NestedBooleanPolicyOptions, NestedBooleanSummary,
    collect_nested_booleans, evaluate_nested_boolean_policy, summarize_nested_booleans,
};
