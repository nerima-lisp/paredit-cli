//! Accessor-arity (an nth/elt/gethash/getf/... accessor with the wrong argument
//! count) detection across explicit files.

pub use crate::domain::accessor_arity_report::{
    AccessorArityItem, AccessorArityPolicy, AccessorArityPolicyOptions, AccessorAritySummary,
    collect_accessor_arity_violations, evaluate_accessor_arity_policy, expected_arity_phrase,
    summarize_accessor_arity,
};
