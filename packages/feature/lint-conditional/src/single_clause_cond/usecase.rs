//! Single-clause-`cond` (`(cond (test body))` is `(when test body)`) detection
//! across explicit files.

pub use crate::single_clause_cond::domain::{
    SingleClauseCondItem, SingleClauseCondPolicy, SingleClauseCondPolicyOptions,
    SingleClauseCondSummary, collect_single_clause_conds, evaluate_single_clause_cond_policy,
    summarize_single_clause_conds,
};
