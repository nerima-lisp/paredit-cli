//! Constant-test `when`/`unless` (`(when t …)` is `(progn …)`) detection.

pub use crate::domain::constant_when_test_report::{
    ConstantWhenTestItem, ConstantWhenTestPolicy, ConstantWhenTestPolicyOptions,
    ConstantWhenTestSummary, collect_constant_when_tests, evaluate_constant_when_test_policy,
    summarize_constant_when_tests,
};
