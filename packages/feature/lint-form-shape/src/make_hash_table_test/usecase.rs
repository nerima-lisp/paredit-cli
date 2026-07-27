//! Make-hash-table-test ((make-hash-table :test 'eql) is (make-hash-table)) detection.

pub use crate::make_hash_table_test::domain::{
    MakeHashTableTestItem, MakeHashTableTestPolicy, MakeHashTableTestPolicyOptions,
    MakeHashTableTestSummary, collect_make_hash_table_tests, evaluate_make_hash_table_test_policy,
    summarize_make_hash_table_tests,
};
