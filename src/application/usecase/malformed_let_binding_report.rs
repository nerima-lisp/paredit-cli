//! Malformed-`let`/`let*`-binding (wrong element count) detection across
//! explicit files.

pub use crate::domain::malformed_let_binding_report::{
    MalformedLetBindingItem, MalformedLetBindingPolicy, MalformedLetBindingPolicyOptions,
    MalformedLetBindingSummary, collect_malformed_let_bindings,
    evaluate_malformed_let_binding_policy, summarize_malformed_let_bindings,
};
