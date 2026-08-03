//! The `macro-body-destroys-argument-form` lint rule: its adapter and
//! detection.
//!
//! One rule, one directory. `rule` is what the registry registers; `domain` is
//! the analysis it drives.

pub mod domain;
pub mod rule;
