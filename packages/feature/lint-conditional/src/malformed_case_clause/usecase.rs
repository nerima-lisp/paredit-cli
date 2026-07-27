//! Malformed-`case`-family-clause (a clause that is not a non-empty list)
//! detection across explicit files.

pub use crate::malformed_case_clause::domain::{
    MalformedCaseClauseItem, MalformedCaseClausePolicy, MalformedCaseClausePolicyOptions,
    MalformedCaseClauseSummary, collect_malformed_case_clauses,
    evaluate_malformed_case_clause_policy, summarize_malformed_case_clauses,
};
