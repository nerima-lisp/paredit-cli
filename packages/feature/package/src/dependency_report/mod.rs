//! One slice, one directory; the layers are names, not directories.
//!
//! No `cli` layer here: dependency_report's command needs
//! definition_report, which belongs to the project-analysis feature and has
//! not been extracted yet, so the cli half stays in the root crate.

pub mod domain;
pub mod usecase;
