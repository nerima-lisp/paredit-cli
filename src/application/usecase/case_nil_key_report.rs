//! Case-`nil`-key (`(case x (nil …))` is a dead clause; use `((nil) …)`)
//! detection across explicit files.

pub use crate::domain::case_nil_key_report::{
    CaseNilKeyItem, CaseNilKeyPolicy, CaseNilKeyPolicyOptions, CaseNilKeySummary,
    collect_case_nil_keys, evaluate_case_nil_key_policy, summarize_case_nil_keys,
};
