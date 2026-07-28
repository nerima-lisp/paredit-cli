//! Duplicate-keyword ((make-instance 'c :x 1 :x 2) passes :x twice) detection.

pub use crate::duplicate_keyword::domain::{
    DuplicateKeywordItem, DuplicateKeywordPolicy, DuplicateKeywordPolicyOptions,
    DuplicateKeywordSummary, collect_duplicate_keywords, evaluate_duplicate_keyword_policy,
    summarize_duplicate_keywords,
};
