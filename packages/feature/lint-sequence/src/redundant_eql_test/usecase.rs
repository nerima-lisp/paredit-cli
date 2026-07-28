//! Redundant `:test #'eql` (`(find x list :test #'eql)` is `(find x list)`)
//! detection across explicit files.

pub use crate::redundant_eql_test::domain::{
    RedundantEqlTestItem, RedundantEqlTestPolicy, RedundantEqlTestPolicyOptions,
    RedundantEqlTestSummary, collect_redundant_eql_tests, evaluate_redundant_eql_test_policy,
    summarize_redundant_eql_tests,
};
