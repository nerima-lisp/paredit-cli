//! Nested-`when` (`(when a (when b body))` is `(when (and a b) body)`) detection
//! across explicit files.

pub use crate::nested_when::domain::{
    NestedWhenItem, NestedWhenPolicy, NestedWhenPolicyOptions, NestedWhenSummary,
    collect_nested_whens, evaluate_nested_when_policy, summarize_nested_whens,
};
