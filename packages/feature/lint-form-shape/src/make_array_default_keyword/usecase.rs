//! Redundant make-array default keyword ((make-array n :adjustable nil) is (make-array n)) detection.

pub use crate::make_array_default_keyword::domain::{
    MakeArrayDefaultKeywordItem, MakeArrayDefaultKeywordPolicy,
    MakeArrayDefaultKeywordPolicyOptions, MakeArrayDefaultKeywordSummary,
    collect_make_array_default_keywords, evaluate_make_array_default_keyword_policy,
    summarize_make_array_default_keywords,
};
