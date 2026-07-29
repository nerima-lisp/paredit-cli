//! Clone detection built on top of the similarity engine.
//!
//! `duplicates` answers "which forms are byte-for-byte the same shape" and
//! `similarity` answers "which two forms are close". Neither answers the
//! questions someone actually removing duplication asks, and this slice is
//! those questions:
//!
//! - `clone-classes` — group the pairs into classes, label each on the standard
//!   clone taxonomy, and rank them by how many lines extracting one would save.
//! - `clone-sequences` — find duplicated *runs of sibling forms*, the case where
//!   only part of a definition was copied and no whole form matches.
//! - `clone-external` — compare this project against a reference corpus, so a
//!   reimplementation of something a dependency already provides shows up.
//! - `clone-threshold` — calibrate `--threshold` from the project's own
//!   similarity distribution instead of the built-in 0.87.
//! - `clone-genealogy` — order a class's members by the commit that introduced
//!   them, which turns "these five are the same" into "this one is the original
//!   and these four are copies".
//!
//! All five share the candidate collection and the tree-edit-distance core with
//! `similarity_report`, which is why they live in the same package.

pub mod cli;
pub mod domain;

// The contract with the composition root (section 4.2): each command publishes
// its `clap` argument type and the function that runs it.
pub use cli::{
    CloneClassReportArgs, CloneExternalReportArgs, CloneGenealogyReportArgs,
    CloneSequenceReportArgs, CloneThresholdReportArgs, clone_classes, clone_external,
    clone_genealogy, clone_sequences, clone_threshold,
};
