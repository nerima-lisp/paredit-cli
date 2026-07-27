//! List*-to-cons ((list* a b) is (cons a b)) detection.

pub use crate::domain::list_star_to_cons_report::{
    ListStarToConsItem, ListStarToConsPolicy, ListStarToConsPolicyOptions, ListStarToConsSummary,
    collect_list_star_to_cons, evaluate_list_star_to_cons_policy, summarize_list_star_to_cons,
};
