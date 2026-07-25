//! Duplicate lambda-list parameter detection across explicit files.

pub use crate::domain::duplicate_parameter_report::{
    DuplicateParameterItem, DuplicateParameterPolicy, DuplicateParameterPolicyOptions,
    DuplicateParameterSummary, collect_duplicate_parameters, evaluate_duplicate_parameter_policy,
    summarize_duplicate_parameters,
};
