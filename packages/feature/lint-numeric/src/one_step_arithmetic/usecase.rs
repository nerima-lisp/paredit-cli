//! One-step-arithmetic (`(+ x 1)` is `(1+ x)`; `(- x 1)` is `(1- x)`) detection
//! across explicit files.

pub use crate::domain::one_step_arithmetic_report::{
    OneStepArithmeticItem, OneStepArithmeticPolicy, OneStepArithmeticPolicyOptions,
    OneStepArithmeticSummary, collect_one_step_arithmetic, evaluate_one_step_arithmetic_policy,
    summarize_one_step_arithmetic,
};
