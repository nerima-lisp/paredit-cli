//! The `paredit config` namespace: inspect and validate the configuration.
//!
//! Separate from `inspect` because it reports on the tool, not on source. An
//! `inspect` command answers a question about a Lisp file; these answer "what
//! is this build going to do, and why".

pub mod args;
mod custom_conflicts;
pub mod render;
pub mod runtime;
pub mod workflow;
