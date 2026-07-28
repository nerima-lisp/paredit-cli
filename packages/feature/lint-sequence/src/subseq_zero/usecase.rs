//! Subseq-zero ((subseq x 0) is (copy-seq x)) detection.

pub use crate::subseq_zero::domain::{
    SubseqZeroItem, SubseqZeroPolicy, SubseqZeroPolicyOptions, SubseqZeroSummary,
    collect_subseq_zeros, evaluate_subseq_zero_policy, summarize_subseq_zeros,
};
