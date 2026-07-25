//! Cross-file same-name/same-category top-level redefinition detection across explicit files.

pub use crate::domain::redefinition_report::{
    DeclaredDefinition, RedefinitionItem, RedefinitionOccurrence, RedefinitionPolicy,
    RedefinitionPolicyOptions, RedefinitionSummary, analyze_redefinitions,
    collect_declared_definitions, evaluate_redefinition_policy,
};
