#![doc = include_str!("../README.md")]

pub mod catalog;
pub mod recipe;
pub mod run;
pub mod scan;

// The contract with the composition root (section 4.2): the slice publishes
// its `clap` argument type and the function that runs it. command.rs and
// dispatch.rs need these two names and no more.
pub use run::cli::{MigrateCommand, migrate};
