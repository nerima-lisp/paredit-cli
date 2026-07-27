//! Sign-comparison (`(= x 0)`/`(> x 0)`/`(< x 0)`, better written with
//! `zerop`/`plusp`/`minusp`) detection across explicit files.

pub use crate::domain::sign_comparison_report::{
    SignComparisonItem, SignComparisonPolicy, SignComparisonPolicyOptions, SignComparisonSummary,
    collect_sign_comparisons, evaluate_sign_comparison_policy, summarize_sign_comparisons,
};
