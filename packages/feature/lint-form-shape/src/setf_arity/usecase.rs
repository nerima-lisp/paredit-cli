//! Odd-arity `setf`/`setq` detection across explicit files.

pub use crate::setf_arity::domain::{
    SetfArityItem, SetfArityPolicy, SetfArityPolicyOptions, SetfAritySummary,
    collect_setf_arity_violations, evaluate_setf_arity_policy, summarize_setf_arity_violations,
};
