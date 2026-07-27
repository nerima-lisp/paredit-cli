//! Redundant getf nil default ((getf p k nil) is (getf p k)) detection.

pub use crate::getf_default_nil::domain::{
    GetfDefaultNilItem, GetfDefaultNilPolicy, GetfDefaultNilPolicyOptions, GetfDefaultNilSummary,
    collect_getf_default_nils, evaluate_getf_default_nil_policy, summarize_getf_default_nils,
};
