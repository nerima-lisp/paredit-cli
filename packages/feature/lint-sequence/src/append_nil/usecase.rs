//! Append-nil ((append x nil) is (copy-list x)) detection.

pub use crate::domain::append_nil_report::{
    AppendNilItem, AppendNilPolicy, AppendNilPolicyOptions, AppendNilSummary, collect_append_nils,
    evaluate_append_nil_policy, summarize_append_nils,
};
