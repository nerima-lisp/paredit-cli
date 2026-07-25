//! Lexical shadowing detection across function parameters and `let`-family
//! bindings.

pub use crate::domain::shadowed_binding_report::{
    ScopeKind, ShadowedBindingItem, ShadowedBindingPolicy, ShadowedBindingPolicyOptions,
    ShadowedBindingReportFile, build_shadowed_binding_report, evaluate_shadowed_binding_policy,
};
