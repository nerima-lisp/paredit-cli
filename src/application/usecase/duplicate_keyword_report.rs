//! Duplicate-keyword ((make-instance 'c :x 1 :x 2) passes :x twice) detection.

pub use crate::domain::duplicate_keyword_report::{
    DuplicateKeywordItem, DuplicateKeywordPolicy, DuplicateKeywordPolicyOptions,
    DuplicateKeywordSummary, collect_duplicate_keywords, evaluate_duplicate_keyword_policy,
    summarize_duplicate_keywords,
};
