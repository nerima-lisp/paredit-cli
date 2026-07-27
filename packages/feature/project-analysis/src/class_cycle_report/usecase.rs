//! CLOS class-inheritance cycle detection across explicit files.

pub use crate::domain::class_cycle_report::{
    ClassCycleItem, ClassCyclePolicy, ClassCyclePolicyOptions, ClassCycleSummary,
    analyze_class_cycles, collect_class_inheritance_edges, evaluate_class_cycle_policy,
};
