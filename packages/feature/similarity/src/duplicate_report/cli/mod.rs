pub mod args;
pub mod render;
pub mod workflow;
pub mod workspace;

// The contract with the composition root (section 4.2): the `clap` argument
// type and the function that runs it.
pub use args::DuplicateReportArgs;
pub use workflow::duplicate_report;
