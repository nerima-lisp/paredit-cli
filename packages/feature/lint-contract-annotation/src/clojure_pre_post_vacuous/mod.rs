//! The `clojure-pre-post-vacuous` lint rule: its detection, use case, and the
//! adapter that registers it.
//!
//! One rule, one directory. `rule` is what the registry registers; the rest is
//! the report it drives.

pub mod domain;
pub mod rule;
pub mod usecase;
