//! Equality-predicate-arity (an eq/eql/equal/equalp call without exactly two
//! arguments) detection across explicit files.

pub use crate::equality_arity::domain::{
    EqualityArityItem, EqualityArityPolicy, EqualityArityPolicyOptions, EqualityAritySummary,
    collect_equality_arity_violations, evaluate_equality_arity_policy, summarize_equality_arity,
};
