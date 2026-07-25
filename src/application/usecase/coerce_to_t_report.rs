//! Coerce-to-t ((coerce x t) is x) detection.

pub use crate::domain::coerce_to_t_report::{
    CoerceToTItem, CoerceToTPolicy, CoerceToTPolicyOptions, CoerceToTSummary, collect_coerce_to_ts,
    evaluate_coerce_to_t_policy, summarize_coerce_to_ts,
};
