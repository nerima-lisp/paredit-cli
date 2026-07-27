//! Redundant-`funcall` (`(funcall #'foo …)`, which is just `(foo …)`) detection
//! across explicit files.

pub use crate::domain::redundant_funcall_report::{
    RedundantFuncallItem, RedundantFuncallPolicy, RedundantFuncallPolicyOptions,
    RedundantFuncallSummary, collect_redundant_funcalls, evaluate_redundant_funcall_policy,
    summarize_redundant_funcalls,
};
