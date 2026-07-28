//! If-to-`or` (`(if x x y)` is `(or x y)`) detection across explicit files.

pub use crate::if_to_or::domain::{
    IfToOrItem, IfToOrPolicy, IfToOrPolicyOptions, IfToOrSummary, collect_if_to_ors,
    evaluate_if_to_or_policy, summarize_if_to_ors,
};
