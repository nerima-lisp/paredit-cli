#![doc = include_str!("../README.md")]

pub mod duplicate_report;
pub mod error;
pub mod form_similarity;
pub mod similarity_report;

// The contract with the composition root (section 4.2): each slice publishes
// its `clap` argument type and the function that runs it. `command.rs` and
// `dispatch.rs` in the root need these two names and nothing else.
pub use duplicate_report::cli::{DuplicateReportArgs, duplicate_report};
pub use similarity_report::cli::{SimilarityReportArgs, similarity_report};

pub use error::{
    SimilarityAnalysisError, SimilarityAnalysisResult, SimilarityBudgetError, SimilarityWorkerError,
};
