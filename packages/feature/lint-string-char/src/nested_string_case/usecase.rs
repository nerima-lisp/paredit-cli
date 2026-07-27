//! Nested string-case (`(OUTER (INNER s))` of two non-destructive string case
//! operations is `(OUTER s)`) detection across explicit files.

pub use crate::nested_string_case::domain::{
    NestedStringCaseItem, NestedStringCasePolicy, NestedStringCasePolicyOptions,
    NestedStringCaseSummary, collect_nested_string_cases, evaluate_nested_string_case_policy,
    summarize_nested_string_cases,
};
