//! `if`-arity (wrong argument count for the `if` special form) detection across
//! explicit files.

pub use crate::if_arity::domain::{
    IfArityItem, IfArityPolicy, IfArityPolicyOptions, IfAritySummary, collect_if_arity_violations,
    evaluate_if_arity_policy, summarize_if_arity,
};
