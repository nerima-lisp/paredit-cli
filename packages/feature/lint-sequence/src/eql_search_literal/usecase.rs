//! Default-`eql`-search-for-a-literal (`(member "x" list)`, `(assoc '(a) al)` —
//! the default eql test never matches a string/list literal) detection across
//! explicit files.

pub use crate::eql_search_literal::domain::{
    EqlSearchLiteralItem, EqlSearchLiteralPolicy, EqlSearchLiteralPolicyOptions,
    EqlSearchLiteralSummary, collect_eql_search_literals, evaluate_eql_search_literal_policy,
    summarize_eql_search_literals,
};
