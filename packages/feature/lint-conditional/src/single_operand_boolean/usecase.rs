//! Single-operand-`and`/`or` (`(and X)`/`(or X)`, which are just `X`) detection
//! across explicit files.

pub use crate::single_operand_boolean::domain::{
    SingleOperandBooleanItem, SingleOperandBooleanPolicy, SingleOperandBooleanPolicyOptions,
    SingleOperandBooleanSummary, collect_single_operand_booleans,
    evaluate_single_operand_boolean_policy, summarize_single_operand_booleans,
};
