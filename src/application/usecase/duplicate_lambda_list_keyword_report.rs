//! Duplicate-lambda-list-keyword (a lambda list repeating `&optional`/`&key`/…)
//! detection across explicit files.

pub use crate::domain::duplicate_lambda_list_keyword_report::{
    DuplicateLambdaListKeywordItem, DuplicateLambdaListKeywordPolicy,
    DuplicateLambdaListKeywordPolicyOptions, DuplicateLambdaListKeywordSummary,
    collect_duplicate_lambda_list_keywords, evaluate_duplicate_lambda_list_keyword_policy,
    summarize_duplicate_lambda_list_keywords,
};
