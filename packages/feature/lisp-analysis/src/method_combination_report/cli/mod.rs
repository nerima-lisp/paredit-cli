pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::MethodCombinationReportArgs;
pub use workflow::method_combination_report;
