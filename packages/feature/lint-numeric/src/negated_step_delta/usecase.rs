//! Negated-step-delta (`(incf x -1)` is `(decf x 1)`) detection across explicit
//! files.

pub use crate::negated_step_delta::domain::{
    NegatedStepDeltaItem, NegatedStepDeltaPolicy, NegatedStepDeltaPolicyOptions,
    NegatedStepDeltaSummary, collect_negated_step_deltas, evaluate_negated_step_delta_policy,
    summarize_negated_step_deltas,
};
