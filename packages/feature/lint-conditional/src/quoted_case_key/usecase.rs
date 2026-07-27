//! Quoted-`case`-key (a case/ecase/ccase clause with a quoted key) detection
//! across explicit files.

pub use crate::domain::quoted_case_key_report::{
    QuotedCaseKeyItem, QuotedCaseKeyPolicy, QuotedCaseKeyPolicyOptions, QuotedCaseKeySummary,
    collect_quoted_case_keys, evaluate_quoted_case_key_policy, summarize_quoted_case_keys,
};
