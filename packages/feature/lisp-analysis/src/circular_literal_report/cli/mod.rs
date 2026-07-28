pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::CircularLiteralReportArgs;
pub use workflow::circular_literal_report;
