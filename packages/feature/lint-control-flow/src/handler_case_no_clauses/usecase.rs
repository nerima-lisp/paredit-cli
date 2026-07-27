//! Handler-case-no-clauses ((handler-case x) is x) detection.

pub use crate::domain::handler_case_no_clauses_report::{
    HandlerCaseNoClausesItem, HandlerCaseNoClausesPolicy, HandlerCaseNoClausesPolicyOptions,
    HandlerCaseNoClausesSummary, collect_handler_case_no_clauses,
    evaluate_handler_case_no_clauses_policy, summarize_handler_case_no_clauses,
};
