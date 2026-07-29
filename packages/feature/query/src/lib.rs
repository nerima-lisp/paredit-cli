#![doc = include_str!("../README.md")]

pub mod count_report;
pub mod find_report;
pub mod replace;
pub mod scan;

// The contract with the composition root (section 4.2): each slice that owns
// a subcommand publishes its `clap` argument type and the function that runs
// it. command.rs and dispatch.rs need these two names and no more.
pub use count_report::cli::{QueryCountArgs, query_count};
pub use find_report::cli::{QueryFindArgs, query_find};
pub use replace::cli::{QueryReplaceArgs, query_replace};
