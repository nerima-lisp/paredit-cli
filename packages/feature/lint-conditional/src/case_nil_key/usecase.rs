//! Case-`nil`-key (`(case x (nil …))` is a dead clause; use `((nil) …)`)
//! detection across explicit files.

pub use crate::case_nil_key::domain::{
    CaseNilKeyItem, CaseNilKeyPolicy, CaseNilKeyPolicyOptions, CaseNilKeySummary,
    collect_case_nil_keys, evaluate_case_nil_key_policy, summarize_case_nil_keys,
};
