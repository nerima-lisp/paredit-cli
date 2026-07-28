//! Negated-`when`/`unless` (a `(not X)`/`(null X)` test) detection across
//! explicit files.

pub use crate::negated_when_unless::domain::{
    NegatedWhenUnlessItem, NegatedWhenUnlessPolicy, NegatedWhenUnlessPolicyOptions,
    NegatedWhenUnlessSummary, collect_negated_when_unless, evaluate_negated_when_unless_policy,
    summarize_negated_when_unless,
};
