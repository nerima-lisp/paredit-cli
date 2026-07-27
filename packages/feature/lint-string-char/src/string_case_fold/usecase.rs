//! String-case-fold ((string= (string-downcase a) (string-downcase b)) is (string-equal a b)) detection.

pub use crate::string_case_fold::domain::{
    StringCaseFoldItem, StringCaseFoldPolicy, StringCaseFoldPolicyOptions, StringCaseFoldSummary,
    collect_string_case_folds, evaluate_string_case_fold_policy, summarize_string_case_folds,
};
