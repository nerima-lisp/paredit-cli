//! Values-list-of-list ((values-list (list a b)) is (values a b)) detection.

pub use crate::domain::values_list_of_list_report::{
    ValuesListOfListItem, ValuesListOfListPolicy, ValuesListOfListPolicyOptions,
    ValuesListOfListSummary, collect_values_list_of_lists, evaluate_values_list_of_list_policy,
    summarize_values_list_of_lists,
};
