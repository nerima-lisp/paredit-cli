//! Redundant-boolean-identity (`t` in `and`, `nil` in `or`, e.g. `(and a t b)`
//! is `(and a b)`) detection across explicit files.

pub use crate::domain::redundant_boolean_identity_report::{
    RedundantBooleanIdentityItem, RedundantBooleanIdentityPolicy,
    RedundantBooleanIdentityPolicyOptions, RedundantBooleanIdentitySummary,
    collect_redundant_boolean_identities, evaluate_redundant_boolean_identity_policy,
    summarize_redundant_boolean_identities,
};
