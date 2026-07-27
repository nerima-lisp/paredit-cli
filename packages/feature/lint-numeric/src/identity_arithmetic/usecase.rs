//! Identity-arithmetic (`(+ x 0)`, `(* x 1)` — a redundant identity operand)
//! detection across explicit files.

pub use crate::identity_arithmetic::domain::{
    IdentityArithmeticItem, IdentityArithmeticPolicy, IdentityArithmeticPolicyOptions,
    IdentityArithmeticSummary, collect_identity_arithmetic, evaluate_identity_arithmetic_policy,
    summarize_identity_arithmetic,
};
