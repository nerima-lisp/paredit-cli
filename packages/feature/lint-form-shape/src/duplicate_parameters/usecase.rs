//! Duplicate lambda-list parameter detection across explicit files.

pub use crate::duplicate_parameters::domain::{
    DuplicateParameterItem, DuplicateParameterPolicy, DuplicateParameterPolicyOptions,
    DuplicateParameterSummary, collect_duplicate_parameters, evaluate_duplicate_parameter_policy,
    summarize_duplicate_parameters,
};
