//! If-not (`(if test nil t)` is `(not test)`) detection.

pub use crate::if_not::domain::{
    IfNotItem, IfNotPolicy, IfNotPolicyOptions, IfNotSummary, collect_if_nots,
    evaluate_if_not_policy, summarize_if_nots,
};
