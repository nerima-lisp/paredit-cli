//! The lint context: what a rule is, how rules are registered, and the single
//! pass that runs them.
//!
//! The suite used to be four hand-maintained parallel arrays plus a 1,650-line
//! function that called 134 collectors in sequence, each walking the whole
//! document on its own, with the matching automatic fixes stranded in the CLI
//! layer. This module inverts that: a rule declares which nodes it wants and
//! what to say about one, [`registry`] is the single list everything else is
//! derived from, and [`engine`] walks the document once for all of them.
//!
//! Layout follows the usual split — [`model`] for the vocabulary, [`policy`]
//! for decisions that need no tree, [`engine`] for the pass, [`registry`] for
//! the catalogue, and [`rules`] for the rules themselves.

pub mod engine;
pub mod model;
pub mod policy;
pub mod registry;
pub mod rule;
pub mod rules;

pub use engine::collect_lint_outcomes;
