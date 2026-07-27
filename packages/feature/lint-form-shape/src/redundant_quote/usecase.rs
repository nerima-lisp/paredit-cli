//! Redundant-quote-of-a-self-evaluating-literal detection across explicit files.

pub use crate::redundant_quote::domain::{
    RedundantQuoteItem, RedundantQuotePolicy, RedundantQuotePolicyOptions, RedundantQuoteSummary,
    collect_redundant_quotes, evaluate_redundant_quote_policy, summarize_redundant_quotes,
};
