//! Constant-`if`-test (`(if t a b)` is `a`, `(if nil a b)` is `b`) detection
//! across explicit files.

pub use crate::domain::constant_if_test_report::{
    ConstantIfTestItem, ConstantIfTestPolicy, ConstantIfTestPolicyOptions, ConstantIfTestSummary,
    collect_constant_if_tests, evaluate_constant_if_test_policy, summarize_constant_if_tests,
};
