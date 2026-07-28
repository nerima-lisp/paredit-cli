pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::LineMetricsReportArgs;
pub use workflow::line_metrics_report;
