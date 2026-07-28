pub mod args;
pub mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2): the argument type and
// run function of each subcommand this slice owns.
pub use args::ComplexityReportArgs;
pub use workflow::complexity_report;
