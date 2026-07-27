//! Cross-file ASDF system name identity conflict detection across explicit files.

pub use crate::domain::system_conflict_report::{
    DeclaredSystem, SystemConflictItem, SystemConflictOccurrence, SystemConflictPolicy,
    SystemConflictPolicyOptions, SystemConflictSummary, analyze_system_conflicts,
    collect_declared_systems, evaluate_system_conflict_policy,
};
