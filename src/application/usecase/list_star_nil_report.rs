//! list* with nil tail ((list* a b nil) is (list a b)) detection.

pub use crate::domain::list_star_nil_report::{
    ListStarNilItem, ListStarNilPolicy, ListStarNilPolicyOptions, ListStarNilSummary,
    collect_list_star_nils, evaluate_list_star_nil_policy, summarize_list_star_nils,
};
