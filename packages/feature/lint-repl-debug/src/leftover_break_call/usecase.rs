//! LeftoverBreakCall (a Common Lisp (break ...) left in committed source) detection.

pub use crate::leftover_break_call::domain::{
    LeftoverBreakCallItem, LeftoverBreakCallPolicy, LeftoverBreakCallPolicyOptions,
    LeftoverBreakCallSummary, collect_leftover_break_call, evaluate_leftover_break_call_policy,
    summarize_leftover_break_call,
};
