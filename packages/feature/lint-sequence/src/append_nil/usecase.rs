//! Append-nil ((append x nil) is (copy-list x)) detection.

pub use crate::append_nil::domain::{
    AppendNilItem, AppendNilPolicy, AppendNilPolicyOptions, AppendNilSummary, collect_append_nils,
    evaluate_append_nil_policy, summarize_append_nils,
};
