//! `if`-arity (wrong argument count for the `if` special form) detection across
//! explicit files.

pub use crate::domain::if_arity_report::{
    IfArityItem, IfArityPolicy, IfArityPolicyOptions, IfAritySummary, collect_if_arity_violations,
    evaluate_if_arity_policy, summarize_if_arity,
};
