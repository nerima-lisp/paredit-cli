//! Extracts a selected form into a new top-level function, inferring its parameters.
//!
//! One slice, one directory: `domain` holds the rules, `usecase` the
//! orchestration, and `cli` the argument parsing and rendering.

pub mod cli;
pub mod domain;
pub mod usecase;
