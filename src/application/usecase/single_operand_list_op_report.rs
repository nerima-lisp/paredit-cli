//! Single-operand list-op (`(append x)` is `x`) detection across explicit
//! files.

pub use crate::domain::single_operand_list_op_report::{
    SingleOperandListOpItem, SingleOperandListOpPolicy, SingleOperandListOpPolicyOptions,
    SingleOperandListOpSummary, collect_single_operand_list_ops,
    evaluate_single_operand_list_op_policy, summarize_single_operand_list_ops,
};
