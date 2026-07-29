//! Synthesizes a `defpackage` form from one file's own definitions and
//! qualified symbol references.
//!
//! One slice, one directory; the layers are names, not directories.

pub mod cli;
pub mod domain;
pub mod usecase;
