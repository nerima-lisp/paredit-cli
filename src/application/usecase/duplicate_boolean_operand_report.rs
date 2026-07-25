//! Duplicate `and`/`or` operand detection across explicit files.

pub use crate::domain::duplicate_boolean_operand_report::{
    DuplicateBooleanOperandItem, DuplicateBooleanOperandPolicy,
    DuplicateBooleanOperandPolicyOptions, DuplicateBooleanOperandSummary,
    collect_duplicate_boolean_operands, evaluate_duplicate_boolean_operand_policy,
    summarize_duplicate_boolean_operands,
};
