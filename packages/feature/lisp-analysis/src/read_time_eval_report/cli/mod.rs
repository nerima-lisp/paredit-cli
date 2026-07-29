pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::ReadTimeEvalReportArgs;
pub use workflow::read_time_eval_report;
