//! Reports near-duplicate forms across a workspace, ranked by similarity.
//!
//! One slice, one directory. The old layers survive as names rather than as
//! directories: `domain` holds the scoring and report model, `usecase` the
//! orchestration behind a source port, and `cli` the argument parsing,
//! workflow and rendering.

pub mod cli;
pub mod domain;
pub mod usecase;
