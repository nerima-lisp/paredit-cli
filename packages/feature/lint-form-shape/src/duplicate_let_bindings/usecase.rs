//! Duplicate parallel-`let` binding detection across explicit files.

pub use crate::duplicate_let_bindings::domain::{
    DuplicateLetBindingItem, DuplicateLetBindingPolicy, DuplicateLetBindingPolicyOptions,
    DuplicateLetBindingSummary, collect_duplicate_let_bindings,
    evaluate_duplicate_let_binding_policy, summarize_duplicate_let_bindings,
};
