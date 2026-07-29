#![doc = include_str!("../README.md")]

pub mod error;
pub mod structural_diff;
pub mod structural_patch;

// The composition root sees each slice's Args type and run fn (section 4.2).
pub use structural_diff::cli::{StructuralDiffArgs, structural_diff};
pub use structural_patch::cli::{StructuralPatchArgs, structural_patch};
