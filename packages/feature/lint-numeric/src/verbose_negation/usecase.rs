//! Verbose-negation (`(- 0 x)`, `(* x -1)` are `(- x)`) detection across
//! explicit files.

pub use crate::verbose_negation::domain::{
    VerboseNegationItem, VerboseNegationPolicy, VerboseNegationPolicyOptions,
    VerboseNegationSummary, collect_verbose_negations, evaluate_verbose_negation_policy,
    summarize_verbose_negations,
};
