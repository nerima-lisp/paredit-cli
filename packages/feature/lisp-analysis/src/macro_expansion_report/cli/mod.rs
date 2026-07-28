pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2).
pub use args::MacroExpansionReportArgs;
pub use workflow::macro_expansion_report;
