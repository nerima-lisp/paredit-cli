//! Typep-predicate ((typep x 'string) is (stringp x)) detection.

pub use crate::domain::typep_predicate_report::{
    TypepPredicateItem, TypepPredicatePolicy, TypepPredicatePolicyOptions, TypepPredicateSummary,
    collect_typep_predicates, evaluate_typep_predicate_policy, summarize_typep_predicates,
};
