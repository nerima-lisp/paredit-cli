//! Call-graph cycle (mutual-recursion) detection across explicit files.

pub use crate::call_cycle_report::domain::{
    CallCycleItem, CallCyclePolicy, CallCyclePolicyOptions, CallCycleSummary, analyze_call_cycles,
    evaluate_call_cycle_policy,
};
