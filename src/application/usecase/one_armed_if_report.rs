//! One-armed-`if` (`(if test then)`, better written `(when test then)`)
//! detection across explicit files.

pub use crate::domain::one_armed_if_report::{
    OneArmedIfItem, OneArmedIfPolicy, OneArmedIfPolicyOptions, OneArmedIfSummary,
    collect_one_armed_ifs, evaluate_one_armed_if_policy, summarize_one_armed_ifs,
};
