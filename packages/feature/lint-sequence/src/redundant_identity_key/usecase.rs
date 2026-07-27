//! Redundant `:key #'identity` (`(sort xs #'< :key #'identity)` is
//! `(sort xs #'<)`) detection across explicit files.

pub use crate::redundant_identity_key::domain::{
    RedundantIdentityKeyItem, RedundantIdentityKeyPolicy, RedundantIdentityKeyPolicyOptions,
    RedundantIdentityKeySummary, collect_redundant_identity_keys,
    evaluate_redundant_identity_key_policy, summarize_redundant_identity_keys,
};
