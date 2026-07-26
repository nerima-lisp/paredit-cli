//! Extracts a selected form into a local `flet`/`labels` binding.
//!
//! One slice, one directory: `domain` holds the rules, `usecase` the
//! orchestration, and `cli` the argument parsing and rendering.

pub mod cli;
pub mod domain;
pub mod usecase;
