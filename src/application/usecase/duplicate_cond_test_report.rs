//! Duplicate `cond`-test detection across explicit files.

pub use crate::domain::duplicate_cond_test_report::{
    DuplicateCondTestItem, DuplicateCondTestPolicy, DuplicateCondTestPolicyOptions,
    DuplicateCondTestSummary, collect_duplicate_cond_tests, evaluate_duplicate_cond_test_policy,
    summarize_duplicate_cond_tests,
};
