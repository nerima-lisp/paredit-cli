//! Synthesizes an ASDF `defsystem` form from a directory of Lisp sources.
//!
//! One slice, one directory; the layers are names, not directories.

pub mod cli;
pub mod domain;
pub mod usecase;
