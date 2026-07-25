//! Redundant getf nil default ((getf p k nil) is (getf p k)) detection.

pub use crate::domain::getf_default_nil_report::{
    GetfDefaultNilItem, GetfDefaultNilPolicy, GetfDefaultNilPolicyOptions, GetfDefaultNilSummary,
    collect_getf_default_nils, evaluate_getf_default_nil_policy, summarize_getf_default_nils,
};
