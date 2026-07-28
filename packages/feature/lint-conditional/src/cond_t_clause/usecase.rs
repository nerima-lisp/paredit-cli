//! Cond-single-t-clause (`(cond (t body…))` is `(progn body…)`) detection.

pub use crate::cond_t_clause::domain::{
    CondTClauseItem, CondTClausePolicy, CondTClausePolicyOptions, CondTClauseSummary,
    collect_cond_t_clauses, evaluate_cond_t_clause_policy, summarize_cond_t_clauses,
};
