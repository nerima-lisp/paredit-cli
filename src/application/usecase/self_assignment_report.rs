//! Self-assignment (`(setq x x)`) detection across explicit files.

pub use crate::domain::self_assignment_report::{
    SelfAssignmentItem, SelfAssignmentPolicy, SelfAssignmentPolicyOptions, SelfAssignmentSummary,
    collect_self_assignments, evaluate_self_assignment_policy, summarize_self_assignments,
};
