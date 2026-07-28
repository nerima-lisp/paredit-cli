//! defstruct :include inheritance cycle detection across explicit files.

pub use crate::struct_cycle_report::domain::{
    StructCycleItem, StructCyclePolicy, StructCyclePolicyOptions, StructCycleSummary,
    analyze_struct_cycles, collect_struct_inheritance_edges, evaluate_struct_cycle_policy,
};
