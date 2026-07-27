//! Redundant-`funcall` (`(funcall #'foo …)`, which is just `(foo …)`) detection
//! across explicit files.

pub use crate::redundant_funcall::domain::{
    RedundantFuncallItem, RedundantFuncallPolicy, RedundantFuncallPolicyOptions,
    RedundantFuncallSummary, collect_redundant_funcalls, evaluate_redundant_funcall_policy,
    summarize_redundant_funcalls,
};
