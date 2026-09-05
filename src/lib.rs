#![doc = include_str!("../README.md")]

// The composition root. Command implementations live in the core and feature
// packages; this crate exposes only the remaining top-level modules.
pub mod lint;
pub mod presentation;
pub mod semantic_coverage;

// Keep these top-level re-exports for the public API contract.
pub use paredit_core_syntax::{dialect, sexpr};
