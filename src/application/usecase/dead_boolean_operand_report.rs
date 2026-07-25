//! Dead-boolean-operand (`(and a nil b)`, `(or a t b)`) detection across explicit files.

pub use crate::domain::dead_boolean_operand_report::{
    DeadBooleanOperandItem, DeadBooleanOperandPolicy, DeadBooleanOperandPolicyOptions,
    DeadBooleanOperandSummary, collect_dead_boolean_operands, evaluate_dead_boolean_operand_policy,
    summarize_dead_boolean_operands,
};
