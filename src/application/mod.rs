//! Application services that orchestrate typed domain operations into
//! agent-facing reports, plans, and refactor workflows.

// Phase 4 facade (section 4.1): application::refactor now lives in
// paredit-feature-refactor-workflow.
pub use paredit_feature_refactor_workflow::refactor::usecase as refactor;
pub mod usecase;
