//! The write side of `inspect constants`.
//!
//! `constant_report`'s own documentation calls its findings "the input a
//! `fold-constants` edit would take"; this is that edit. It takes the spans
//! and reader spellings that report already computes rather than folding
//! anything itself, so the two can never disagree about what is foldable.

pub mod cli;

pub use cli::{FoldConstantsArgs, fold_constants};
