//! Binds-constant (a let/let*/do/do* binding of nil, t, or a keyword) detection
//! across explicit files.

pub use crate::binds_constant::domain::{
    BindsConstantItem, BindsConstantPolicy, BindsConstantPolicyOptions, BindsConstantSummary,
    collect_binds_constant, evaluate_binds_constant_policy, summarize_binds_constant,
};
