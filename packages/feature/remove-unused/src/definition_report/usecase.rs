//! Definition inventory and unused-definition analysis.

mod collect;
#[cfg(test)]
mod tests;
mod types;

pub use crate::domain::definition_report::{
    collect_unused_definition_candidates, evaluate_unused_definition_policy,
    unused_definition_actionable_candidate_count, unused_definition_candidate_count,
};
pub use collect::{
    build_definition_report, build_parsed_definition_file, collect_definition_forms,
};
pub use types::{
    DefinitionReference, DefinitionReportFile, DefinitionReportItem, ParsedDefinitionFile,
    UnusedDefinitionFile, UnusedDefinitionItem, UnusedDefinitionPolicy,
    UnusedDefinitionPolicyOptions,
};
