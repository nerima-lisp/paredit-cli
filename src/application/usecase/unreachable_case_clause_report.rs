//! Unreachable-`case`/`typecase`-clause (clauses stranded after a `t`/`otherwise`
//! catch-all) detection across explicit files.

pub use crate::domain::unreachable_case_clause_report::{
    UnreachableCaseClauseItem, UnreachableCaseClausePolicy, UnreachableCaseClausePolicyOptions,
    UnreachableCaseClauseSummary, collect_unreachable_case_clauses,
    evaluate_unreachable_case_clause_policy, summarize_unreachable_case_clauses,
};
