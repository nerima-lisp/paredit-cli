#![doc = include_str!("../README.md")]

pub mod accumulation_discarded_by_finally_return;
pub mod into_accumulator_never_read;
pub mod parallel_binding_reads_sibling;

pub mod loop_grammar;
pub mod shared;

#[cfg(test)]
mod tests;

// This crate is deliberately **unregistered**. The root's REGISTRY does not
// name these rules yet and the `const` assertions in
// `src/lint/registry/catalog.rs` therefore do not count them; a separate pass
// wires them, which is also where the pinned rule counts and the lint goldens
// move together.
