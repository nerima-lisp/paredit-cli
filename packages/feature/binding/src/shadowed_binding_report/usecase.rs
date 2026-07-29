//! Lexical shadowing detection across function parameters and `let`-family
//! bindings.

pub use crate::shadowed_binding_report::domain::{
    ScopeKind, ShadowedBindingItem, ShadowedBindingPolicy, ShadowedBindingPolicyOptions,
    ShadowedBindingReportFile, build_shadowed_binding_report, evaluate_shadowed_binding_policy,
};
