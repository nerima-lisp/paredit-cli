//! Reports exact duplicate forms across a workspace.
//!
//! Shares `form_similarity` with `similarity_report`, which is why the two
//! live in one package rather than two.

pub mod cli;
pub mod domain;
pub mod usecase;
