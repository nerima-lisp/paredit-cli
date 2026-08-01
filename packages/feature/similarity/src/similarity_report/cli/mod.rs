pub mod args;
pub mod cache;
pub mod render;
pub mod types;
pub mod workflow;

// The contract with the composition root (section 4.2): the `clap` argument
// type and the function that runs it.
pub use args::SimilarityReportArgs;
pub use workflow::similarity_report;
