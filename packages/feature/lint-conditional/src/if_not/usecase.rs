//! If-not (`(if test nil t)` is `(not test)`) detection.

pub use crate::domain::if_not_report::{
    IfNotItem, IfNotPolicy, IfNotPolicyOptions, IfNotSummary, collect_if_nots,
    evaluate_if_not_policy, summarize_if_nots,
};
