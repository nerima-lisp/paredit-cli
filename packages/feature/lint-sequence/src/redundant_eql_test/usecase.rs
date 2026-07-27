//! Redundant `:test #'eql` (`(find x list :test #'eql)` is `(find x list)`)
//! detection across explicit files.

pub use crate::domain::redundant_eql_test_report::{
    RedundantEqlTestItem, RedundantEqlTestPolicy, RedundantEqlTestPolicyOptions,
    RedundantEqlTestSummary, collect_redundant_eql_tests, evaluate_redundant_eql_test_policy,
    summarize_redundant_eql_tests,
};
