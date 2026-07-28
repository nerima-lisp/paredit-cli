//! Single-operand-`+`/`*` (`(+ X)`/`(* X)`, which are just `X`) detection
//! across explicit files.

pub use crate::single_operand_arithmetic::domain::{
    SingleOperandArithmeticItem, SingleOperandArithmeticPolicy,
    SingleOperandArithmeticPolicyOptions, SingleOperandArithmeticSummary,
    collect_single_operand_arithmetic, evaluate_single_operand_arithmetic_policy,
    summarize_single_operand_arithmetic,
};
