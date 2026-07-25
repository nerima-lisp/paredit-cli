//! `the`-arity (a `the` special form without exactly two arguments) detection
//! across explicit files.

pub use crate::domain::the_arity_report::{
    TheArityItem, TheArityPolicy, TheArityPolicyOptions, TheAritySummary,
    collect_the_arity_violations, evaluate_the_arity_policy, summarize_the_arity,
};
