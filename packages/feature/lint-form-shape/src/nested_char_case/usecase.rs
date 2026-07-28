//! Nested char case ((char-upcase (char-downcase c)) is (char-upcase c)) detection.

pub use crate::nested_char_case::domain::{
    NestedCharCaseItem, NestedCharCasePolicy, NestedCharCasePolicyOptions, NestedCharCaseSummary,
    collect_nested_char_cases, evaluate_nested_char_case_policy, summarize_nested_char_cases,
};
