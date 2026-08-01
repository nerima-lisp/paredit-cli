//! SelfRecursiveTailCall (a definition's own name called in tail position of its body, annotated with whether the target dialect guarantees tail-call optimization there) detection.

pub use crate::self_recursive_tail_call::domain::{
    SelfRecursiveTailCallItem, SelfRecursiveTailCallPolicy, SelfRecursiveTailCallPolicyOptions,
    SelfRecursiveTailCallSummary, collect_self_recursive_tail_call,
    evaluate_self_recursive_tail_call_policy, summarize_self_recursive_tail_call,
};
