//! Nthcdr-zero (`(nthcdr 0 list)` is `list`) detection across explicit files.

pub use crate::nthcdr_zero::domain::{
    NthcdrZeroItem, NthcdrZeroPolicy, NthcdrZeroPolicyOptions, NthcdrZeroSummary,
    collect_nthcdr_zeros, evaluate_nthcdr_zero_policy, summarize_nthcdr_zeros,
};
