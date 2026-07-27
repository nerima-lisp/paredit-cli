//! Step-zero ((incf x 0) is a no-op) detection.

pub use crate::domain::step_zero_report::{
    StepZeroItem, StepZeroPolicy, StepZeroPolicyOptions, StepZeroSummary, collect_step_zeros,
    evaluate_step_zero_policy, summarize_step_zeros,
};
