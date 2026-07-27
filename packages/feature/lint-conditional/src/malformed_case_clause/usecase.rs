//! Malformed-`case`-family-clause (a clause that is not a non-empty list)
//! detection across explicit files.

pub use crate::domain::malformed_case_clause_report::{
    MalformedCaseClauseItem, MalformedCaseClausePolicy, MalformedCaseClausePolicyOptions,
    MalformedCaseClauseSummary, collect_malformed_case_clauses,
    evaluate_malformed_case_clause_policy, summarize_malformed_case_clauses,
};
