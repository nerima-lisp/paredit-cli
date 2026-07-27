//! Empty-`let` (`(let () body)` is `(progn body)`) detection across explicit
//! files.

pub use crate::empty_let::domain::{
    EmptyLetItem, EmptyLetPolicy, EmptyLetPolicyOptions, EmptyLetSummary, collect_empty_lets,
    evaluate_empty_let_policy, summarize_empty_lets,
};
