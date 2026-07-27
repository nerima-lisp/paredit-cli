//! Missing-`format`-destination (`(format "…" …)` — a string literal where the
//! destination belongs) detection across explicit files.

pub use crate::format_missing_destination::domain::{
    FormatMissingDestinationItem, FormatMissingDestinationPolicy,
    FormatMissingDestinationPolicyOptions, FormatMissingDestinationSummary,
    collect_format_missing_destinations, evaluate_format_missing_destination_policy,
    summarize_format_missing_destinations,
};
