//! Reorders top-level definitions by name, preserving comments.
//!
//! One slice, one directory; the layers are names, not directories.
//!
//! No `cli` layer: this slice has no subcommand of its own and is driven
//! by another command's workflow.

pub mod domain;
pub mod usecase;
