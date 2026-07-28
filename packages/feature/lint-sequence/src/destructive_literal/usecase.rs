//! Destructive-operation-on-a-literal (`(nreverse '(a b))`, `(sort '(1 2) …)` —
//! undefined behavior) detection across explicit files.

pub use crate::destructive_literal::domain::{
    DestructiveLiteralItem, DestructiveLiteralPolicy, DestructiveLiteralPolicyOptions,
    DestructiveLiteralSummary, collect_destructive_literals, evaluate_destructive_literal_policy,
    summarize_destructive_literals,
};
