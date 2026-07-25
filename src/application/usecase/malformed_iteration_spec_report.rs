//! Malformed-iteration-spec (a `dolist`/`dotimes` spec that is not
//! `(var form [result])`) detection across explicit files.

pub use crate::domain::malformed_iteration_spec_report::{
    MalformedIterationSpecItem, MalformedIterationSpecPolicy, MalformedIterationSpecPolicyOptions,
    MalformedIterationSpecSummary, collect_malformed_iteration_specs,
    evaluate_malformed_iteration_spec_policy, summarize_malformed_iteration_specs,
};
