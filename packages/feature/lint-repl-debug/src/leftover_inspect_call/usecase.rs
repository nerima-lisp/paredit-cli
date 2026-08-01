//! LeftoverInspectCall (a Common Lisp (inspect x) or (describe x) left in committed source) detection.

pub use crate::leftover_inspect_call::domain::{
    LeftoverInspectCallItem, LeftoverInspectCallPolicy, LeftoverInspectCallPolicyOptions,
    LeftoverInspectCallSummary, collect_leftover_inspect_call,
    evaluate_leftover_inspect_call_policy, summarize_leftover_inspect_call,
};
