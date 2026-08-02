//! The `docstring-example-stale-arity` lint rule: its detection and its
//! adapter.
//!
//! One rule, one directory. `rule` is what the registry registers; `domain` is
//! the detection it drives. See
//! [`crate::docstring_summary_line_too_long`] for why there is no
//! `usecase`/`cli` pair.

pub mod domain;
pub mod rule;
