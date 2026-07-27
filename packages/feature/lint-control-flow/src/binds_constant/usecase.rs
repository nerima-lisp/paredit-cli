//! Binds-constant (a let/let*/do/do* binding of nil, t, or a keyword) detection
//! across explicit files.

pub use crate::domain::binds_constant_report::{
    BindsConstantItem, BindsConstantPolicy, BindsConstantPolicyOptions, BindsConstantSummary,
    collect_binds_constant, evaluate_binds_constant_policy, summarize_binds_constant,
};
