//! Modify-macro-arity (an incf/decf/push/pop with the wrong argument count)
//! detection across explicit files.

pub use crate::domain::modify_macro_arity_report::{
    ModifyMacroArityItem, ModifyMacroArityPolicy, ModifyMacroArityPolicyOptions,
    ModifyMacroAritySummary, collect_modify_macro_arity_violations,
    evaluate_modify_macro_arity_policy, expected_arity_phrase, summarize_modify_macro_arity,
};
