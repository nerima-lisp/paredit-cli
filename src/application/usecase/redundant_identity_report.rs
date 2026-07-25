//! Redundant-`identity` (`(identity x)`, which is just `x`) detection across
//! explicit files.

pub use crate::domain::redundant_identity_report::{
    RedundantIdentityItem, RedundantIdentityPolicy, RedundantIdentityPolicyOptions,
    RedundantIdentitySummary, collect_redundant_identities, evaluate_redundant_identity_policy,
    summarize_redundant_identities,
};
