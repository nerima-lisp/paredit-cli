//! Typecase-`nil`-key (`(typecase x (nil …))` is a dead clause; use `(null …)`)
//! detection across explicit files.

pub use crate::typecase_nil_key::domain::{
    TypecaseNilKeyItem, TypecaseNilKeyPolicy, TypecaseNilKeyPolicyOptions, TypecaseNilKeySummary,
    collect_typecase_nil_keys, evaluate_typecase_nil_key_policy, summarize_typecase_nil_keys,
};
