//! Char-case-fold ((char= (char-downcase a) (char-downcase b)) is (char-equal a b)) detection.

pub use crate::domain::char_case_fold_report::{
    CharCaseFoldItem, CharCaseFoldPolicy, CharCaseFoldPolicyOptions, CharCaseFoldSummary,
    collect_char_case_folds, evaluate_char_case_fold_policy, summarize_char_case_folds,
};
