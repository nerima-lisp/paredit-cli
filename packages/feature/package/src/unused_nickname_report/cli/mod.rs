pub mod args;
mod render;
pub mod workflow;

// Hoisted for the composition root (section 4.2): the argument type and
// run function of each subcommand this slice owns.
pub use args::UnusedNicknameReportArgs;
pub use workflow::unused_nickname_report;
