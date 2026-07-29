pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2): the argument type and
// run function of the subcommand this slice owns.
pub use args::DuplicateSlotReportArgs;
pub use workflow::duplicate_slot_report;
