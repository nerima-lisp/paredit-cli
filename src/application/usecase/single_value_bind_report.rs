//! Single-value `multiple-value-bind` (`(multiple-value-bind (x) f body)` is
//! `(let ((x f)) body)`) detection across explicit files.

pub use crate::domain::single_value_bind_report::{
    SingleValueBindItem, SingleValueBindPolicy, SingleValueBindPolicyOptions,
    SingleValueBindSummary, collect_single_value_binds, evaluate_single_value_bind_policy,
    summarize_single_value_binds,
};
