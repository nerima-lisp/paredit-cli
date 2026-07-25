//! Unreachable-`cond`-clause (clauses stranded after a `t` catch-all) detection
//! across explicit files.

pub use crate::domain::unreachable_cond_clause_report::{
    UnreachableCondClauseItem, UnreachableCondClausePolicy, UnreachableCondClausePolicyOptions,
    UnreachableCondClauseSummary, collect_unreachable_cond_clauses,
    evaluate_unreachable_cond_clause_policy, summarize_unreachable_cond_clauses,
};
