pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::UnreachableExpressionReportArgs;
pub use workflow::unreachable_expression_report;
