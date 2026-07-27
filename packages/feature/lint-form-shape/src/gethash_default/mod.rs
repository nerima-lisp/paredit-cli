//! The `gethash-default` lint rule: its adapter, detection, use case and command.
//!
//! One rule, one directory. `rule` is what the registry registers; the
//! rest is the report it drives.

pub mod cli;
pub mod domain;
pub mod rule;
pub mod usecase;
