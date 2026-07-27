//! Step-zero ((incf x 0) is a no-op) detection.

pub use crate::step_zero::domain::{
    StepZeroItem, StepZeroPolicy, StepZeroPolicyOptions, StepZeroSummary, collect_step_zeros,
    evaluate_step_zero_policy, summarize_step_zeros,
};
