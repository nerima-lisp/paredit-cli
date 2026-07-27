//! Redundant :start 0 ((find x seq :start 0) is (find x seq)) detection.

pub use crate::redundant_start_zero::domain::{
    RedundantStartZeroItem, RedundantStartZeroPolicy, RedundantStartZeroPolicyOptions,
    RedundantStartZeroSummary, collect_redundant_start_zeros, evaluate_redundant_start_zero_policy,
    summarize_redundant_start_zeros,
};
