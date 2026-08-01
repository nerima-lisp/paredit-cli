pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::MagicNumberReportArgs;
pub use workflow::magic_number_report;
