pub mod args;
pub mod workflow;

mod render;

// Hoisted for the composition root (section 4.2): the argument type and
// run function of each subcommand this slice owns.
pub use args::FormReportArgs;
pub use workflow::form_report;
