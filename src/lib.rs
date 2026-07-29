#![doc = include_str!("../README.md")]

// The composition root, and nothing else. Everything a command actually does
// lives in `packages/core/*` and `packages/feature/*`; what is left here is
// what cannot live in either.
//
// There is deliberately no `domain`, `application` or `infrastructure` module.
// They existed as 415 lines of `pub use` re-exporting other packages, 26 of
// which anything referenced — and a directory named `domain` is where "just
// put it here for now" goes. Seven report modules had accumulated in one.
pub mod lint;
pub mod presentation;
pub mod semantic_coverage;

// The two vocabulary modules this crate's own integration tests read the world
// through, re-exported from `paredit-core-syntax`, which owns them. See
// `tests/cli/public_api_docs_contract.rs` for why the `paredit_cli::` spelling
// is kept at all.
pub use paredit_core_syntax::{dialect, sexpr};
