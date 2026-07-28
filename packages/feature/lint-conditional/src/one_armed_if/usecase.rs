//! One-armed-`if` (`(if test then)`, better written `(when test then)`)
//! detection across explicit files.

pub use crate::one_armed_if::domain::{
    OneArmedIfItem, OneArmedIfPolicy, OneArmedIfPolicyOptions, OneArmedIfSummary,
    collect_one_armed_ifs, evaluate_one_armed_if_policy, summarize_one_armed_ifs,
};
