#![doc = include_str!("../README.md")]

pub mod engine;
pub mod error;
pub mod model;
pub mod policy;
pub mod rule;

pub use error::{LintError, LintResult, RuleSelectionError};
