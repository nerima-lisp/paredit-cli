//! Backwards-compatible application facade for similarity analysis.
pub use crate::domain::similarity_report::*;

pub mod types;
pub mod workflow;

pub use types::{
    DiscoveredSimilarityFile, InvalidSimilarityReportPlan, SimilarityDuplicatePolicy,
    SimilarityErrorPolicy, SimilarityFileError, SimilarityGateDecision,
    SimilarityIndeterminateReason, SimilarityInventory, SimilarityProcessingStage,
    SimilarityReportPlan, SimilarityReportRequest, SimilarityReportSourcePort,
    SimilarityReportWorkflowError,
};
pub use workflow::build_similarity_report;

#[cfg(test)]
mod tests;
