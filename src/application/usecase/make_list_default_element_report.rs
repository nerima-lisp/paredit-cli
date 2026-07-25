//! Redundant :initial-element nil ((make-list n :initial-element nil) is (make-list n)) detection.

pub use crate::domain::make_list_default_element_report::{
    MakeListDefaultElementItem, MakeListDefaultElementPolicy, MakeListDefaultElementPolicyOptions,
    MakeListDefaultElementSummary, collect_make_list_default_elements,
    evaluate_make_list_default_element_policy, summarize_make_list_default_elements,
};
