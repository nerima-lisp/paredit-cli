//! Equality-predicate-arity (an eq/eql/equal/equalp call without exactly two
//! arguments) detection across explicit files.

pub use crate::domain::equality_arity_report::{
    EqualityArityItem, EqualityArityPolicy, EqualityArityPolicyOptions, EqualityAritySummary,
    collect_equality_arity_violations, evaluate_equality_arity_policy, summarize_equality_arity,
};
