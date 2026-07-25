//! Malformed-`cond`-clause (a clause that is not a non-empty list) detection
//! across explicit files.

pub use crate::domain::malformed_cond_clause_report::{
    MalformedCondClauseItem, MalformedCondClausePolicy, MalformedCondClausePolicyOptions,
    MalformedCondClauseSummary, collect_malformed_cond_clauses,
    evaluate_malformed_cond_clause_policy, summarize_malformed_cond_clauses,
};
