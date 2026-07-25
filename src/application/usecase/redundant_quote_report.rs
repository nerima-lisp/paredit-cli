//! Redundant-quote-of-a-self-evaluating-literal detection across explicit files.

pub use crate::domain::redundant_quote_report::{
    RedundantQuoteItem, RedundantQuotePolicy, RedundantQuotePolicyOptions, RedundantQuoteSummary,
    collect_redundant_quotes, evaluate_redundant_quote_policy, summarize_redundant_quotes,
};
