pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::ApiDiffReportArgs;
pub use workflow::api_diff_report;
