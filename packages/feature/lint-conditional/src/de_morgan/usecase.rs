//! De Morgan (`(and (not a) (not b))` is `(not (or a b))`) detection across
//! explicit files.

pub use crate::domain::de_morgan_report::{
    DeMorganItem, DeMorganPolicy, DeMorganPolicyOptions, DeMorganSummary, collect_de_morgans,
    evaluate_de_morgan_policy, summarize_de_morgans,
};
