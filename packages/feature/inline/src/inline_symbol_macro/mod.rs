//! Replaces a symbol macro with its expansion.
//!
//! One slice, one directory: `domain` holds the rules, `usecase` the
//! orchestration, and `cli` the argument parsing and rendering.

pub mod cli;
pub mod domain;
pub mod usecase;
