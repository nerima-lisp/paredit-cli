//! Duplicate `case`/`ecase`/`ccase` key detection across explicit files.

pub use crate::duplicate_case_keys::domain::{
    DuplicateCaseKeyItem, DuplicateCaseKeyPolicy, DuplicateCaseKeyPolicyOptions,
    DuplicateCaseKeySummary, collect_duplicate_case_keys, evaluate_duplicate_case_key_policy,
    summarize_duplicate_case_keys,
};
