//! Negated-`if` (`(if (not c) a b)`, better written `(if c b a)`) detection
//! across explicit files.

pub use crate::domain::negated_if_report::{
    NegatedIfItem, NegatedIfPolicy, NegatedIfPolicyOptions, NegatedIfSummary, collect_negated_ifs,
    evaluate_negated_if_policy, summarize_negated_ifs,
};
