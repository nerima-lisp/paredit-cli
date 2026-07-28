//! Append-list-to-cons ((append (list x) rest) is (cons x rest)) detection.

pub use crate::append_list_to_cons::domain::{
    AppendListToConsItem, AppendListToConsPolicy, AppendListToConsPolicyOptions,
    AppendListToConsSummary, collect_append_list_to_cons, evaluate_append_list_to_cons_policy,
    summarize_append_list_to_cons,
};
