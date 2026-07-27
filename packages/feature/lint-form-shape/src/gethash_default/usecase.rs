//! Gethash-default ((gethash k h nil) is (gethash k h)) detection.

pub use crate::gethash_default::domain::{
    GethashDefaultItem, GethashDefaultPolicy, GethashDefaultPolicyOptions, GethashDefaultSummary,
    collect_gethash_defaults, evaluate_gethash_default_policy, summarize_gethash_defaults,
};
