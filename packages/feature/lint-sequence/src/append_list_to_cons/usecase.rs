//! Append-list-to-cons ((append (list x) rest) is (cons x rest)) detection.

pub use crate::domain::append_list_to_cons_report::{
    AppendListToConsItem, AppendListToConsPolicy, AppendListToConsPolicyOptions,
    AppendListToConsSummary, collect_append_list_to_cons, evaluate_append_list_to_cons_policy,
    summarize_append_list_to_cons,
};
