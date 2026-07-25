//! Redundant butlast count of 1 ((butlast x 1) is (butlast x)) detection.

pub use crate::domain::butlast_default_count_report::{
    ButlastDefaultCountItem, ButlastDefaultCountPolicy, ButlastDefaultCountPolicyOptions,
    ButlastDefaultCountSummary, collect_butlast_default_counts,
    evaluate_butlast_default_count_policy, summarize_butlast_default_counts,
};
