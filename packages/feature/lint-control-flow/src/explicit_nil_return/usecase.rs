//! Explicit-nil-return (`(return nil)` is `(return)`) detection across explicit
//! files.

pub use crate::explicit_nil_return::domain::{
    ExplicitNilReturnItem, ExplicitNilReturnPolicy, ExplicitNilReturnPolicyOptions,
    ExplicitNilReturnSummary, collect_explicit_nil_returns, evaluate_explicit_nil_return_policy,
    summarize_explicit_nil_returns,
};
