#![doc = include_str!("../README.md")]

pub mod resolve_report;

// The contract with the composition root (section 4.2): each slice that owns
// a subcommand publishes its `clap` argument type and the function that runs
// it. command.rs and dispatch.rs need these two names and no more.
pub use resolve_report::cli::{ResolveReportArgs, resolve_report};
