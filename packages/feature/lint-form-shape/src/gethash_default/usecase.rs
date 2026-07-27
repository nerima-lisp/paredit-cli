//! Gethash-default ((gethash k h nil) is (gethash k h)) detection.

pub use crate::domain::gethash_default_report::{
    GethashDefaultItem, GethashDefaultPolicy, GethashDefaultPolicyOptions, GethashDefaultSummary,
    collect_gethash_defaults, evaluate_gethash_default_policy, summarize_gethash_defaults,
};
