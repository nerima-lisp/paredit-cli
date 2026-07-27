//! Sign-comparison (`(= x 0)`/`(> x 0)`/`(< x 0)`, better written with
//! `zerop`/`plusp`/`minusp`) detection across explicit files.

pub use crate::sign_comparison::domain::{
    SignComparisonItem, SignComparisonPolicy, SignComparisonPolicyOptions, SignComparisonSummary,
    collect_sign_comparisons, evaluate_sign_comparison_policy, summarize_sign_comparisons,
};
