//! Cross-file same-name/same-category top-level redefinition detection across explicit files.

pub use crate::redefinition_report::domain::{
    DeclaredDefinition, RedefinitionItem, RedefinitionOccurrence, RedefinitionPolicy,
    RedefinitionPolicyOptions, RedefinitionSummary, analyze_redefinitions,
    collect_declared_definitions, evaluate_redefinition_policy,
};
