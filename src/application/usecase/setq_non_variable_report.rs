//! Setq-non-variable (a setq/psetq place that is not a variable) detection
//! across explicit files.

pub use crate::domain::setq_non_variable_report::{
    SetqNonVariableItem, SetqNonVariablePolicy, SetqNonVariablePolicyOptions,
    SetqNonVariableSummary, collect_setq_non_variables, evaluate_setq_non_variable_policy,
    summarize_setq_non_variables,
};
