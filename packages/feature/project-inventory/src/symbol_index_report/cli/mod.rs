pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::SymbolIndexReportArgs;
pub use workflow::symbol_index_report;
