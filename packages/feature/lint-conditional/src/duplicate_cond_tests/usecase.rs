//! Duplicate `cond`-test detection across explicit files.

pub use crate::duplicate_cond_tests::domain::{
    DuplicateCondTestItem, DuplicateCondTestPolicy, DuplicateCondTestPolicyOptions,
    DuplicateCondTestSummary, collect_duplicate_cond_tests, evaluate_duplicate_cond_test_policy,
    summarize_duplicate_cond_tests,
};
