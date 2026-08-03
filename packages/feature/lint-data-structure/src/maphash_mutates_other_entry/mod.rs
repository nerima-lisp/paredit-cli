//! The `maphash-mutates-other-entry` lint rule: its adapter and detection.
//!
//! One rule, one directory. `rule` is what the registry registers; `domain` is
//! the analysis it drives, and also what a standalone `inspect` command would
//! call if one is wired later.

pub mod domain;
pub mod rule;
