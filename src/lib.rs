#![doc = include_str!("../README.md")]

pub mod application;
pub mod domain;
pub mod presentation;

// The two vocabulary modules this crate's own integration tests read the world
// through. Sourced from `paredit-core-syntax`, which owns them, rather than
// from a `domain` module that used to re-export 200 names it did not own —
// see `tests/cli/public_api_docs_contract.rs` for why the `paredit_cli::`
// spelling is kept at all.
pub use paredit_core_syntax::{dialect, sexpr};
