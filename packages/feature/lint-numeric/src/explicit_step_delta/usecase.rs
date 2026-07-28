//! Explicit-step-delta (`(incf x 1)` is `(incf x)`) detection across explicit
//! files.

pub use crate::explicit_step_delta::domain::{
    ExplicitStepDeltaItem, ExplicitStepDeltaPolicy, ExplicitStepDeltaPolicyOptions,
    ExplicitStepDeltaSummary, collect_explicit_step_deltas, evaluate_explicit_step_delta_policy,
    summarize_explicit_step_deltas,
};
