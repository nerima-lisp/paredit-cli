pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::ValuePropagationReportArgs;
pub use workflow::value_propagation_report;
